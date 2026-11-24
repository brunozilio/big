#![allow(dead_code, clippy::unwrap_used, clippy::expect_used)]

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use async_trait::async_trait;
use connectors::contracts::{self, code, pipelines, tracker};
use connectors::oauth::Dialect;
use connectors::{
    AccountOption, ActionError, AuthSpec, Connectors, Credentials, DeviceApp, EventBatch, EventSource, ExternalEvent,
    FakeHttp, FixedClock, Grant, Http, MemoryTokenStore, Provider, ProviderDescriptor, TokenKeeper, TokenStore,
};
use domain::connection::{Capability, ConnectionId, ProviderKey};
use domain::effect::{Call, Effect, EffectOutcome, EffectRequest, IdempotencyKey};
use domain::workflow::{ItemId, WorkItem, WorkflowEvent};
use serde_json::{Value, json};
use store::{ConnectionRecord, Store};
use sync::{EventSink, StoreView, SyncConfig, Synchronizer};

#[derive(Debug, Default)]
pub struct World {
    pub batches: VecDeque<Result<Vec<ExternalEvent>, ActionError>>,
    pub cursors: Vec<Option<String>>,
    pub issues: HashMap<String, (String, bool)>,
    pub pulls: HashMap<String, String>,
    pub runs: HashMap<String, String>,
    pub calls: Vec<(String, Value, String)>,
    pub fail: Option<ActionError>,
}

#[derive(Debug, Default)]
pub struct Fake {
    world: Mutex<World>,
}

impl Fake {
    pub fn leaked() -> &'static Fake {
        Box::leak(Box::new(Fake::default()))
    }

    pub fn world(&self) -> MutexGuard<'_, World> {
        self.world.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub fn writes(&self) -> Vec<(String, Value, String)> {
        let writes = [tracker::TRANSITION, tracker::SET_IMPEDIMENT];
        self.world().calls.iter().filter(|(action, _, _)| writes.contains(&action.as_str())).cloned().collect()
    }
}

fn answer(world: &mut World, action: &str, args: &Value) -> Result<Value, ActionError> {
    let text = |name: &str| args[name].as_str().unwrap_or_default().to_owned();
    match action {
        tracker::ISSUE => {
            let (status, flagged) = world.issues.get(&text("issue")).cloned().unwrap_or_default();
            Ok(
                json!({"ref": text("issue"), "title": "", "status": status, "impediment": {"on": flagged, "reason": null}}),
            )
        }
        tracker::TRANSITION => {
            let entry = world.issues.entry(text("issue")).or_default();
            let changed = entry.0 != text("status");
            entry.0 = text("status");
            Ok(json!({"ref": text("issue"), "status": entry.0, "changed": changed}))
        }
        tracker::SET_IMPEDIMENT => {
            let on = args["on"].as_bool().unwrap_or_default();
            world.issues.entry(text("issue")).or_default().1 = on;
            Ok(json!({"on": on}))
        }
        code::PULL_REQUEST => {
            let subject = text("pr");
            let state = world.pulls.get(&subject).cloned().unwrap_or_else(|| "open".into());
            Ok(json!({"ref": subject, "state": state, "head": "h", "base": "main", "url": ""}))
        }
        pipelines::RUN => {
            let id = text("run");
            let state = world.runs.get(&id).cloned().unwrap_or_else(|| "running".into());
            Ok(json!({"ref": id, "state": state, "head": "h", "attempt": 1, "jobs": []}))
        }
        _ => Err(ActionError::permanent(format!("fake does not answer {action}"))),
    }
}

#[async_trait]
impl Provider for Fake {
    fn descriptor(&self) -> ProviderDescriptor {
        ProviderDescriptor {
            key: ProviderKey::from("fake"),
            name: "Fake".into(),
            logo: "<svg/>".into(),
            capabilities: [contracts::TRACKER, contracts::CODE, contracts::PIPELINES].map(Capability::from).to_vec(),
            auth: AuthSpec::Device(DeviceApp {
                client_id: "c".into(),
                client_secret: None,
                device_endpoint: "https://login.test/device".into(),
                token_endpoint: "https://login.test/token".into(),
                scopes: Vec::new(),
                start_fields: Vec::new(),
                dialect: Dialect::Form,
            }),
            actions: [contracts::tracker(), contracts::code(), contracts::pipelines()].concat(),
        }
    }

    async fn call(
        &self,
        _http: &dyn Http,
        _creds: &Credentials,
        action: &str,
        args: &Value,
        key: &str,
    ) -> Result<Value, ActionError> {
        let mut world = self.world();
        if let Some(error) = world.fail.clone() {
            return Err(error);
        }
        world.calls.push((action.to_owned(), args.clone(), key.to_owned()));
        answer(&mut world, action, args)
    }

    async fn accounts(&self, _http: &dyn Http, _creds: &Credentials) -> Result<Vec<AccountOption>, ActionError> {
        Ok(Vec::new())
    }

    fn events(&self) -> Option<&dyn EventSource> {
        Some(self)
    }
}

#[async_trait]
impl EventSource for Fake {
    async fn poll(
        &self,
        _http: &dyn Http,
        creds: &Credentials,
        cursor: Option<&str>,
    ) -> Result<EventBatch, ActionError> {
        let mut world = self.world();
        world.cursors.push(cursor.map(str::to_owned));
        if let Some(error) = world.fail.clone() {
            return Err(error);
        }
        let next = cursor.and_then(|text| text.parse::<u64>().ok()).unwrap_or(0) + 1;
        let events = world.batches.pop_front().unwrap_or(Ok(Vec::new()))?;
        let events = events.into_iter().filter(|event| event.connection == creds.connection).collect();
        Ok(EventBatch { events, cursor: next.to_string() })
    }
}

#[derive(Debug)]
pub struct FakeSink {
    store: Arc<Store>,
    pub waiting: Mutex<Vec<String>>,
    pub triggers: Mutex<Vec<String>>,
    pub delivered: Mutex<Vec<ExternalEvent>>,
    pub triggered: Mutex<Vec<ExternalEvent>>,
    pub recorded: Mutex<Vec<(ItemId, WorkflowEvent)>>,
    pub refuse: Mutex<Option<String>>,
}

fn held<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

impl FakeSink {
    pub fn new(store: Arc<Store>) -> FakeSink {
        FakeSink {
            store,
            waiting: Mutex::default(),
            triggers: Mutex::default(),
            delivered: Mutex::default(),
            triggered: Mutex::default(),
            recorded: Mutex::default(),
            refuse: Mutex::default(),
        }
    }

    pub fn wait_on(&self, subject: &str) {
        held(&self.waiting).push(subject.to_owned());
    }

    pub fn trigger_on(&self, kind: &str) {
        held(&self.triggers).push(kind.to_owned());
    }

    pub fn delivered(&self) -> Vec<ExternalEvent> {
        held(&self.delivered).clone()
    }

    pub fn triggered(&self) -> Vec<ExternalEvent> {
        held(&self.triggered).clone()
    }

    pub fn recorded(&self) -> Vec<(ItemId, WorkflowEvent)> {
        held(&self.recorded).clone()
    }

    pub fn reconciled(&self) -> Vec<(String, String)> {
        self.recorded()
            .into_iter()
            .filter_map(|(_, event)| match event {
                WorkflowEvent::Reconciled { field, action, .. } => Some((field, action)),
                _ => None,
            })
            .collect()
    }
}

#[async_trait]
impl EventSink for FakeSink {
    async fn deliver(&self, event: &ExternalEvent) -> Result<usize, sync::Error> {
        if held(&self.refuse).as_deref() == Some(event.id.as_str()) {
            return Err(sync::Error::Sink("kernel is down".into()));
        }
        if !held(&self.waiting).contains(&event.subject) {
            return Ok(0);
        }
        held(&self.delivered).push(event.clone());
        Ok(1)
    }

    async fn trigger(&self, event: &ExternalEvent) -> Result<usize, sync::Error> {
        if !held(&self.triggers).contains(&event.kind) {
            return Ok(0);
        }
        held(&self.triggered).push(event.clone());
        Ok(1)
    }

    async fn record(&self, item: &ItemId, events: &[WorkflowEvent]) -> Result<(), sync::Error> {
        self.store.append(item, events)?;
        held(&self.recorded).extend(events.iter().map(|event| (item.clone(), event.clone())));
        Ok(())
    }
}

pub const TRACKER_CONNECTION: &str = "trk";
pub const HOST_CONNECTION: &str = "host";

pub fn config() -> SyncConfig {
    SyncConfig::from_values("10", "100", "5", "2", "30")
}

pub struct Harness {
    pub fake: &'static Fake,
    pub store: Arc<Store>,
    pub sink: Arc<FakeSink>,
    pub view: Arc<StoreView>,
    pub tokens: Arc<MemoryTokenStore>,
    pub sync: Synchronizer,
}

fn record(id: &str, capabilities: &[&str]) -> ConnectionRecord {
    ConnectionRecord {
        id: id.into(),
        provider: "fake".into(),
        label: id.into(),
        account: None,
        capabilities: capabilities.iter().map(|c| (*c).to_owned()).collect(),
        settings: json!({}),
    }
}

pub async fn harness_on(store: Arc<Store>, fake: &'static Fake) -> Harness {
    harness_with(store, fake, config()).await
}

pub async fn harness_with(store: Arc<Store>, fake: &'static Fake, config: SyncConfig) -> Harness {
    store.save_connection(&record(TRACKER_CONNECTION, &[contracts::TRACKER])).unwrap();
    store.save_connection(&record(HOST_CONNECTION, &[contracts::CODE, contracts::PIPELINES])).unwrap();
    let tokens = Arc::new(MemoryTokenStore::new());
    for id in [TRACKER_CONNECTION, HOST_CONNECTION] {
        tokens.save(&ConnectionId::from(id), &Grant::bearer("tok")).await.unwrap();
    }
    let keeper = TokenKeeper::new(tokens.clone(), Arc::new(FakeHttp::new()), Arc::new(FixedClock::at_second(1_000)));
    let provider: &'static dyn Provider = fake;
    let connectors = Arc::new(Connectors::with_providers(&[provider], Arc::new(FakeHttp::new()), keeper));
    let sink = Arc::new(FakeSink::new(store.clone()));
    let view = Arc::new(StoreView::new(store.clone()));
    let sync = Synchronizer::new(store.clone(), connectors, sink.clone(), view.clone(), config);
    Harness { fake, store, sink, view, tokens, sync }
}

pub async fn harness() -> Harness {
    harness_on(Arc::new(Store::in_memory(0).unwrap()), Fake::leaked()).await
}

impl Harness {
    pub fn item(&self, id: &str) -> WorkItem {
        self.store.item(&ItemId::from(id)).unwrap().unwrap()
    }

    pub fn add(&self, id: &str, events: &[WorkflowEvent]) {
        self.store.append(&ItemId::from(id), events).unwrap();
    }

    pub fn sync_connectors(&self) -> Arc<Connectors> {
        self.sync.connectors().clone()
    }

    pub fn queue(&self, batch: Result<Vec<ExternalEvent>, ActionError>) {
        self.fake.world().batches.push_back(batch);
    }
}

pub fn created(id: &str, input: Value) -> WorkflowEvent {
    WorkflowEvent::ItemCreated { id: ItemId::from(id), workflow: "flow".into(), parent: None, input }
}

pub fn call(item: &str, seq: u64, action: &str, connection: &str, args: Value, value: Value) -> [WorkflowEvent; 2] {
    let key = IdempotencyKey::new(item, "s", None, seq, 1);
    let effect = Effect::ConnectorCall(Call {
        capability: action.split('.').next().unwrap_or_default().into(),
        action: action.into(),
        connection: Some(connection.into()),
        args,
    });
    [
        WorkflowEvent::EffectRequested(EffectRequest::new(key.clone(), effect)),
        WorkflowEvent::EffectResolved { key, outcome: EffectOutcome::Ok { value } },
    ]
}

pub fn wait(item: &str, seq: u64, kind: &str, filter: Value) -> WorkflowEvent {
    let key = IdempotencyKey::new(item, "s", None, seq, 1);
    WorkflowEvent::EffectRequested(EffectRequest::new(
        key,
        Effect::WaitEvent { kind: kind.into(), filter, timeout: None },
    ))
}

pub fn standard_item(h: &Harness, id: &str) {
    let mut events = vec![created(id, json!({"issue": "K-1"})), WorkflowEvent::StatusSet("Doing".into())];
    events.extend(call(
        id,
        1,
        code::OPEN_PULL_REQUEST,
        HOST_CONNECTION,
        json!({"head": "h"}),
        json!({"ref": "o/r#7", "number": 7, "state": "open"}),
    ));
    h.add(id, &events);
    let mut world = h.fake.world();
    world.issues.insert("K-1".into(), ("Doing".into(), false));
    world.pulls.insert("o/r#7".into(), "open".into());
}

pub fn event(id: &str, connection: &str, kind: &str, subject: &str, data: Value, second: i64) -> ExternalEvent {
    ExternalEvent {
        id: id.into(),
        connection: ConnectionId::from(connection),
        capability: Capability::from(kind.split('.').next().unwrap_or_default()),
        kind: kind.into(),
        subject: subject.into(),
        data,
        at: jiff::Timestamp::from_second(second).unwrap(),
    }
}
