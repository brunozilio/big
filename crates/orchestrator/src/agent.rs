use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use agent_engine::EventSink;
use domain::effect::{ErrorClass, IdempotencyKey};
use domain::workflow::{ItemId, WorkflowEvent};
use domain::{AgentAsk, EngineEvent, ModelOption, PermissionMode, PromptRequest, RunId, SessionCommand, SessionId};
use serde_json::{Value, json};
use tokio::sync::mpsc;

use crate::asks::{PendingAsk, SharedRun};
use crate::events::OrchestratorEvent;
use crate::git;
use crate::protocol::Command;
use crate::shared::{Shared, now_ms};

pub const RESUME: &str = "A execução anterior foi interrompida. Continue de onde parou e termine a tarefa.";

pub const WAITING_ANSWER: &str = "o agente espera uma resposta";

#[derive(Debug, Clone, PartialEq)]
pub enum Failure {
    Class(ErrorClass, String),
    Blocked(String),
}

impl Failure {
    pub fn permanent(message: impl Into<String>) -> Failure {
        Failure::Class(ErrorClass::Permanent, message.into())
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
struct Profile {
    mode: Option<PermissionMode>,
    model: Option<String>,
    tools: Vec<String>,
    body: String,
}

impl Profile {
    fn parse(text: &str) -> Profile {
        let (header, body) = agent_engine::frontmatter::parse(text);
        let tools = header
            .get("tools")
            .map(|list| {
                list.trim_matches(|letter| letter == '[' || letter == ']')
                    .split(',')
                    .map(|tool| tool.trim().trim_matches('"').to_owned())
                    .filter(|tool| !tool.is_empty())
                    .collect()
            })
            .unwrap_or_default();
        Profile {
            mode: header.get("permissionmode").and_then(domain::command::mode_named),
            model: header.get("model").map(str::to_owned),
            tools,
            body: body.trim().to_owned(),
        }
    }
}

pub async fn blocking<T: Send + 'static>(work: impl FnOnce() -> T + Send + 'static) -> Result<T, Failure> {
    tokio::task::spawn_blocking(work).await.map_err(|error| Failure::Class(ErrorClass::Transient, error.to_string()))
}

pub async fn workdir(shared: &Shared, branch: &str) -> Result<PathBuf, Failure> {
    let (repo, data, branch) = (shared.config.repo.clone(), shared.config.data.clone(), branch.to_owned());
    blocking(move || git::worktree(repo.as_deref(), &data, &branch))
        .await?
        .map_err(|error| Failure::Class(ErrorClass::Transient, format!("the worktree could not be made: {error}")))
}

pub fn branch_of(command: &Command, item: &ItemId) -> String {
    command.args["worktree"]
        .as_str()
        .filter(|name| !name.is_empty())
        .map_or_else(|| git::branch_name(item.as_str()), str::to_owned)
}

fn model(shared: &Shared, wanted: &str) -> (ModelOption, bool) {
    let models = shared.config.engine.models();
    let named = |id: &str| ModelOption {
        id: id.to_owned(),
        label: id.to_owned(),
        provider: Default::default(),
        window: None,
        capabilities: None,
    };
    if wanted == "auto" {
        return (models.first().cloned().unwrap_or_else(|| named(wanted)), true);
    }
    (models.iter().find(|option| option.id == wanted).cloned().unwrap_or_else(|| named(wanted)), false)
}

fn prompt_of(input: &Value) -> String {
    match input.get("prompt").and_then(Value::as_str) {
        Some(prompt) => prompt.to_owned(),
        None => input.to_string(),
    }
}

fn request(shared: &Shared, profile: &Profile, command: &Command, cwd: &Path, resume: Option<&str>) -> PromptRequest {
    let input = &command.args["input"];
    let wanted = command.args["model"].as_str().or(profile.model.as_deref()).unwrap_or("auto");
    let (option, auto) = model(shared, wanted);
    let carried = input.get("session").and_then(Value::as_str);
    let text = if resume.is_some() { RESUME.to_owned() } else { prompt_of(input) };
    let mut request = PromptRequest::plain(cwd.to_path_buf(), text, option);
    request.session.auto_pick = auto;
    if let Some(effort) = command.args["effort"].as_str().and_then(domain::Effort::named) {
        request.session.effort = effort;
    }
    request.session.resume = resume.or(carried).map(|id| SessionId(id.to_owned()));
    request.session.append_system = Some(profile.body.clone()).filter(|body| !body.is_empty());
    let mode = command.args["mode"].as_str().and_then(domain::command::mode_named).or(profile.mode);
    if let Some(mode) = mode {
        request.policy.permission = mode;
    }
    request
}

#[derive(Debug, Default)]
struct Collected {
    text: String,
    session: Option<String>,
    output: Value,
    checkpoint: Option<u32>,
}

struct Running {
    run: SharedRun,
    done: bool,
}

impl Drop for Running {
    fn drop(&mut self) {
        if !self.done {
            self.run.lock().unwrap_or_else(PoisonError::into_inner).cancel();
        }
    }
}

pub struct AgentEffect<'a> {
    pub shared: &'a Arc<Shared>,
    pub item: &'a ItemId,
    pub command: &'a Command,
    pub key: &'a IdempotencyKey,
    pub resume: Option<String>,
}

impl AgentEffect<'_> {
    pub async fn run(self) -> Result<Value, Failure> {
        let name = self.command.text("name");
        let definition = self
            .shared
            .config
            .resources
            .agent(&name)
            .ok_or_else(|| Failure::permanent(format!("there is no agent named {name}")))?;
        let profile = Profile::parse(&definition.text);
        let branch = branch_of(self.command, self.item);
        let cwd = workdir(self.shared, &branch).await?;
        let place = cwd.clone();
        let base = blocking(move || git::head(&place)).await?;
        let request = request(self.shared, &profile, self.command, &cwd, self.resume.as_deref());
        let (sender, receiver) = mpsc::unbounded_channel();
        let sink: Arc<dyn EventSink> = Arc::new(move |_run: RunId, event: EngineEvent| {
            let _ = sender.send(event);
        });
        let started = self.shared.config.engine.start(RunId::new(), &request, sink);
        let handle = started.map_err(|error| Failure::permanent(error.to_string()))?;
        let mut running = Running { run: Arc::new(Mutex::new(handle)), done: false };
        self.reopen(&running.run);
        let followed = self.follow(receiver, &running.run).await;
        self.close_asks();
        let collected = followed?;
        running.done = true;
        let place = cwd.clone();
        let pushed = branch.clone();
        let commit = blocking(move || {
            let commit = git::head(&place);
            git::push(&place, &pushed);
            commit
        })
        .await?;
        Ok(json!({
            "text": collected.text, "session": collected.session, "output": collected.output,
            "checkpoint": collected.checkpoint, "base": base, "commit": commit, "branch": branch, "agent": name,
            "worktree": cwd.to_string_lossy(),
        }))
    }

    fn ask_limit(&self) -> Duration {
        self.command.args["askTimeout"].as_u64().map_or(self.shared.config.limits.ask_timeout, Duration::from_millis)
    }

    async fn follow(
        &self,
        mut receiver: mpsc::UnboundedReceiver<EngineEvent>,
        run: &SharedRun,
    ) -> Result<Collected, Failure> {
        let mut collected = Collected::default();
        let mut impeded = false;
        loop {
            let deadline = self.ask_deadline(impeded);
            let event = tokio::select! {
                event = receiver.recv() => event,
                () = sleep_for(deadline) => {
                    impeded = true;
                    self.impede(true);
                    continue;
                }
            };
            let Some(event) = event else {
                return Err(Failure::Class(ErrorClass::Transient, "the agent run ended without finishing".into()));
            };
            if let Some(ended) = self.take(event, &mut collected, run) {
                if impeded {
                    self.impede(false);
                }
                return ended.map(|()| collected);
            }
            if impeded && self.shared.asks.of_key(self.key).is_empty() {
                impeded = false;
                self.impede(false);
            }
        }
    }

    fn ask_deadline(&self, impeded: bool) -> Option<Duration> {
        if impeded {
            return None;
        }
        let since = self.shared.asks.of_key(self.key).iter().map(|ask| ask.since).min()?;
        let limit = i64::try_from(self.ask_limit().as_millis()).unwrap_or(i64::MAX);
        let left = since.saturating_add(limit).saturating_sub(now_ms());
        Some(Duration::from_millis(u64::try_from(left).unwrap_or(0)))
    }

    fn impede(&self, on: bool) {
        let reason = on.then(|| WAITING_ANSWER.to_owned());
        let current = self.shared.item(self.item).ok().and_then(|item| item.impediment);
        let ours = current.as_ref().is_some_and(|found| found.reason.as_deref() == Some(WAITING_ANSWER));
        if on == ours || (!on && current.is_none()) {
            return;
        }
        let _ = self.shared.append(self.item, &[WorkflowEvent::ImpedimentSet { on, reason }]);
    }

    fn take(&self, event: EngineEvent, collected: &mut Collected, run: &SharedRun) -> Option<Result<(), Failure>> {
        match event {
            EngineEvent::SessionStarted(session) => self.started(session.0, collected),
            EngineEvent::Checkpoint(index) => {
                collected.checkpoint.get_or_insert(index);
            }
            EngineEvent::TextDelta(piece) => collected.text.push_str(&piece),
            EngineEvent::StructuredOutput(value) => collected.output = value,
            EngineEvent::Asked(ask) => self.asked(ask, collected, run),
            EngineEvent::AskWithdrawn(ask) | EngineEvent::Answered { ask, .. } => {
                if self.shared.asks.close(&ask).is_some() {
                    self.note(json!({ "asks": { ask.0.clone(): null } }));
                    self.shared.emit(OrchestratorEvent::AskClosed { item: self.item.clone(), ask });
                }
            }
            EngineEvent::Completed { .. } => return Some(Ok(())),
            EngineEvent::Failed(message) => return Some(Err(Failure::permanent(message))),
            EngineEvent::Failure(kind) => return Some(Err(Failure::permanent(format!("{kind:?}")))),
            EngineEvent::Cancelled => return Some(Err(Failure::permanent("the agent run was cancelled"))),
            _ => {}
        }
        None
    }

    fn started(&self, session: String, collected: &mut Collected) {
        if collected.session.as_deref() == Some(session.as_str()) {
            return;
        }
        collected.session = Some(session.clone());
        let data = json!({ "session": session, "agent": self.command.text("name") });
        let _ = self.shared.append(self.item, &[WorkflowEvent::EffectProgress { key: self.key.clone(), data }]);
        self.shared.emit(OrchestratorEvent::Session { item: self.item.clone(), step: self.key.step.clone(), session });
    }

    fn close_asks(&self) {
        let gone = self.shared.asks.forget(self.key);
        if !gone.is_empty() {
            self.note(json!({ "asks": null }));
        }
        for pending in gone {
            self.shared.emit(OrchestratorEvent::AskClosed { item: self.item.clone(), ask: pending.ask.id });
        }
    }

    fn note(&self, data: Value) {
        let progress = WorkflowEvent::EffectProgress { key: self.key.clone(), data };
        if let Err(error) = self.shared.append(self.item, &[progress]) {
            tracing::warn!(%error, "an ask could not be kept in the log");
        }
    }

    fn announce(&self, pending: PendingAsk) {
        self.shared.emit(OrchestratorEvent::Asked {
            item: pending.item,
            step: pending.step,
            session: pending.session,
            ask: pending.ask,
        });
    }

    fn reopen(&self, run: &SharedRun) {
        for pending in self.shared.asks.attach(self.key, run) {
            self.announce(pending);
        }
    }

    fn asked(&self, ask: AgentAsk, collected: &Collected, run: &SharedRun) {
        if let Some(decision) = self.shared.asks.take_early(self.key, &ask.kind) {
            run.lock().unwrap_or_else(PoisonError::into_inner).answer(&ask.id, &decision);
            return;
        }
        let pending = PendingAsk {
            item: self.item.clone(),
            step: self.key.step.clone(),
            session: collected.session.clone(),
            ask: ask.clone(),
            since: now_ms(),
        };
        let replaced = self.shared.asks.open(pending.clone(), self.key.clone(), Arc::clone(run));
        let mut asks = serde_json::Map::new();
        for gone in replaced.iter().filter(|gone| gone.ask.id != ask.id) {
            asks.insert(gone.ask.id.0.clone(), Value::Null);
            self.shared.emit(OrchestratorEvent::AskClosed { item: self.item.clone(), ask: gone.ask.id.clone() });
        }
        asks.insert(ask.id.0.clone(), serde_json::to_value(&pending).unwrap_or(Value::Null));
        self.note(json!({ "asks": asks }));
        self.announce(pending);
    }
}

async fn sleep_for(duration: Option<Duration>) {
    match duration {
        Some(duration) => tokio::time::sleep(duration).await,
        None => std::future::pending().await,
    }
}

pub async fn rewind_session(shared: &Shared, cwd: PathBuf, session: &str, checkpoint: u32) -> bool {
    let (option, _) = model(shared, "auto");
    let mut request = PromptRequest::plain(cwd, String::new(), option);
    request.session.resume = Some(SessionId(session.to_owned()));
    request.session.command = Some(SessionCommand::Rewind(checkpoint));
    let (sender, mut receiver) = mpsc::unbounded_channel();
    let sink: Arc<dyn EventSink> = Arc::new(move |_run: RunId, event: EngineEvent| {
        let _ = sender.send(event);
    });
    let Ok(handle) = shared.config.engine.start(RunId::new(), &request, sink) else { return false };
    let _running = Running { run: Arc::new(Mutex::new(handle)), done: true };
    let waited = tokio::time::timeout(Duration::from_secs(60), async {
        while let Some(event) = receiver.recv().await {
            match event {
                EngineEvent::Completed { .. } | EngineEvent::Rewound(_) => return true,
                EngineEvent::Failed(_) | EngineEvent::Failure(_) | EngineEvent::Cancelled => return false,
                _ => {}
            }
        }
        false
    });
    waited.await.unwrap_or(false)
}
