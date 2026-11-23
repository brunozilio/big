use std::collections::HashMap;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock, PoisonError};
use std::time::{SystemTime, UNIX_EPOCH};

use domain::effect::{EffectOutcome, IdempotencyKey};
use domain::workflow::{ItemId, WorkItem, WorkflowEvent};
use platform::CancellationToken;
use serde_json::Value;
use store::Store;
use tokio::sync::{Semaphore, broadcast, mpsc};
use tokio::task::AbortHandle;

use crate::activation::Script;
use crate::asks::Asks;
use crate::cloud::Leases;
use crate::config::Config;
use crate::error::{Error, Result};
use crate::events::{OrchestratorEvent, Wake};
use crate::repos::{self, Checkout};
use crate::resources::Resources;

type ScriptKey = (Option<PathBuf>, String);

#[derive(Debug, Clone, PartialEq)]
pub struct EventWait {
    pub item: ItemId,
    pub key: IdempotencyKey,
    pub kind: String,
    pub filter: Value,
}

#[derive(Debug)]
pub struct Shared {
    pub config: Config,
    pub store: Arc<Store>,
    pub events: broadcast::Sender<OrchestratorEvent>,
    pub asks: Asks,
    pub waits: Mutex<Vec<EventWait>>,
    pub permits: Arc<Semaphore>,
    pub token: CancellationToken,
    pub sync: OnceLock<Arc<sync::Synchronizer>>,
    pub clouds: Leases,
    runners: Mutex<HashMap<ItemId, (u64, mpsc::UnboundedSender<Wake>)>>,
    generation: AtomicU64,
    scripts: Mutex<HashMap<ScriptKey, Arc<Script>>>,
    pins: Mutex<HashMap<(String, String), Arc<Script>>>,
    appending: Mutex<()>,
    pub observed: Mutex<Vec<ItemId>>,
    pub observing: tokio::sync::Notify,
}

pub fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| i64::try_from(elapsed.as_millis()).unwrap_or(i64::MAX))
}

pub fn timeout_timer(key: &IdempotencyKey) -> String {
    format!("timeout:{key}")
}

impl Shared {
    pub fn new(config: Config) -> Shared {
        let (events, _) = broadcast::channel(1024);
        Shared {
            store: Arc::clone(&config.store),
            permits: Arc::new(Semaphore::new(config.limits.max_parallel_items)),
            config,
            events,
            asks: Asks::default(),
            waits: Mutex::default(),
            token: CancellationToken::new(),
            sync: OnceLock::new(),
            clouds: Leases::default(),
            runners: Mutex::default(),
            generation: AtomicU64::new(0),
            scripts: Mutex::default(),
            pins: Mutex::default(),
            appending: Mutex::default(),
            observed: Mutex::default(),
            observing: tokio::sync::Notify::new(),
        }
    }

    pub fn emit(&self, event: OrchestratorEvent) {
        let _ = self.events.send(event);
    }

    pub fn append(&self, item: &ItemId, events: &[WorkflowEvent]) -> Result<WorkItem> {
        let _guard = self.appending.lock().unwrap_or_else(PoisonError::into_inner);
        self.append_locked(item, events)
    }

    fn append_locked(&self, item: &ItemId, events: &[WorkflowEvent]) -> Result<WorkItem> {
        let state = self.store.append(item, events)?;
        if !events.is_empty() {
            self.emit(OrchestratorEvent::ItemChanged { item: item.clone(), version: state.version });
        }
        if self.config.factory.enabled && crate::factory::relevant(events) {
            let mut observed = self.observed.lock().unwrap_or_else(PoisonError::into_inner);
            if !observed.contains(item) {
                observed.push(item.clone());
            }
            self.observing.notify_one();
        }
        Ok(state)
    }

    pub fn item(&self, item: &ItemId) -> Result<WorkItem> {
        self.store.item(item)?.ok_or_else(|| Error::UnknownItem(item.to_string()))
    }

    pub fn resolve(&self, item: &ItemId, key: &IdempotencyKey, outcome: EffectOutcome) -> Result<bool> {
        let _guard = self.appending.lock().unwrap_or_else(PoisonError::into_inner);
        let state = self.item(item)?;
        let open = state.effects.get(&key.to_string()).is_some_and(|entry| entry.outcome.is_none());
        if !open || state.is_ended() {
            return Ok(false);
        }
        let mut events = vec![WorkflowEvent::EffectResolved { key: key.clone(), outcome }];
        for timer in [key.to_string(), timeout_timer(key)] {
            if state.timers.contains_key(&timer) {
                events.push(WorkflowEvent::TimerFired { id: timer });
            }
        }
        self.append_locked(item, &events)?;
        self.wake(item, Wake::Poke);
        Ok(true)
    }

    pub fn wake(&self, item: &ItemId, wake: Wake) -> bool {
        let runners = self.runners.lock().unwrap_or_else(PoisonError::into_inner);
        runners.get(item).is_some_and(|(_, sender)| sender.send(wake).is_ok())
    }

    pub fn register(&self, item: &ItemId) -> Option<(u64, mpsc::UnboundedReceiver<Wake>)> {
        let mut runners = self.runners.lock().unwrap_or_else(PoisonError::into_inner);
        if runners.get(item).is_some_and(|(_, sender)| !sender.is_closed()) {
            return None;
        }
        let generation = self.generation.fetch_add(1, Ordering::Relaxed);
        let (sender, receiver) = mpsc::unbounded_channel();
        runners.insert(item.clone(), (generation, sender));
        Some((generation, receiver))
    }

    pub fn unregister(&self, item: &ItemId, generation: u64) {
        let mut runners = self.runners.lock().unwrap_or_else(PoisonError::into_inner);
        if runners.get(item).is_some_and(|(known, _)| *known == generation) {
            runners.remove(item);
        }
    }

    pub fn spawn(&self, work: impl Future<Output = ()> + Send + 'static) -> AbortHandle {
        let token = self.token.clone();
        tokio::spawn(async move {
            tokio::select! {
                () = token.cancelled() => {}
                () = work => {}
            }
        })
        .abort_handle()
    }

    pub fn checkouts(&self, item: &WorkItem) -> Vec<Checkout> {
        repos::of_input(&crate::application::live(&self.store, &item.input), self.config.repo.as_deref())
    }

    pub fn home(&self, input: &Value) -> Option<PathBuf> {
        repos::home(&crate::application::live(&self.store, input), self.config.repo.as_deref())
    }

    fn other_home<'a>(&self, home: Option<&'a Path>) -> Option<&'a Path> {
        home.filter(|home| self.config.repo.as_deref() != Some(*home))
    }

    pub fn resources_in(&self, home: Option<&Path>) -> Resources {
        match self.other_home(home) {
            Some(home) => self.config.resources.in_repo(home),
            None => self.config.resources.clone(),
        }
    }

    pub fn script(&self, workflow: &str) -> Result<Arc<Script>> {
        self.script_in(None, workflow)
    }

    pub fn script_for(&self, item: &WorkItem) -> Result<Arc<Script>> {
        if let Some((version, code)) = self.store.pinned_workflow(item.id.as_str())? {
            return self.pinned(&version, &code, &item.workflow);
        }
        let script = self.script_in(self.home(&item.input).as_deref(), &item.workflow)?;
        self.store.pin_workflow(item.id.as_str(), &script.version(), &script.code())?;
        Ok(script)
    }

    pub fn latest_for(&self, item: &WorkItem) -> Result<Arc<Script>> {
        self.script_in(self.home(&item.input).as_deref(), &item.workflow)
    }

    pub fn adopt(&self, item: &WorkItem, script: &Script) -> Result<()> {
        self.store.pin_workflow(item.id.as_str(), &script.version(), &script.code())?;
        Ok(())
    }

    fn pinned(&self, version: &str, code: &str, workflow: &str) -> Result<Arc<Script>> {
        let key = (version.to_owned(), workflow.to_owned());
        if let Some(found) = self.pins.lock().unwrap_or_else(PoisonError::into_inner).get(&key) {
            return Ok(Arc::clone(found));
        }
        let loaded = Arc::new(Script::from_code(code, workflow).map_err(Error::Script)?);
        let mut pins = self.pins.lock().unwrap_or_else(PoisonError::into_inner);
        Ok(Arc::clone(pins.entry(key).or_insert(loaded)))
    }

    pub fn script_in(&self, home: Option<&Path>, workflow: &str) -> Result<Arc<Script>> {
        let home = self.other_home(home);
        let key = (home.map(Path::to_path_buf), workflow.to_owned());
        if let Some(found) = self.scripts.lock().unwrap_or_else(PoisonError::into_inner).get(&key) {
            return Ok(Arc::clone(found));
        }
        let loaded = Arc::new(Script::load(&self.resources_in(home), workflow).map_err(Error::Script)?);
        let mut scripts = self.scripts.lock().unwrap_or_else(PoisonError::into_inner);
        Ok(Arc::clone(scripts.entry(key).or_insert(loaded)))
    }

    pub fn forget_script(&self, workflow: &str) {
        self.scripts.lock().unwrap_or_else(PoisonError::into_inner).retain(|(_, name), _| name != workflow);
    }

    pub fn hurry(&self) {
        if let Some(sync) = self.sync.get() {
            sync.hurry();
        }
    }

    pub fn forget_waits(&self, item: &ItemId, key: Option<&IdempotencyKey>) {
        let mut waits = self.waits.lock().unwrap_or_else(PoisonError::into_inner);
        waits.retain(|wait| &wait.item != item || key.is_some_and(|key| &wait.key != key));
    }
}
