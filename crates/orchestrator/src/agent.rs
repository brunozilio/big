use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use agent_engine::EventSink;
use agent_engine::tools::builtin::schedules;
use agent_engine::tools::clients::{self, Waiting};
use domain::effect::{ErrorClass, IdempotencyKey};
use domain::workflow::{ItemId, WorkflowEvent};
use domain::{
    AgentAsk, AskId, AskKind, Decision, EngineEvent, FailureKind, MainAgentSpec, ModelOption, PermissionMode, Prompt,
    PromptRequest, RunId, SessionCommand, SessionId,
};
use serde_json::{Value, json};
use tokio::sync::mpsc;

use crate::asks::{PendingAsk, SharedRun};
use crate::events::OrchestratorEvent;
use crate::git;
use crate::protocol::Command;
use crate::repos::{self, Checkout, Worktree};
use crate::shared::{Shared, now_ms};

pub const RESUME: &str = "The previous run was interrupted. Continue where it stopped and finish the task. If a question or a plan went unanswered because of the interruption, ask again, calling the same tool with the same questions or the same plan, and wait for the answer. Never decide in the user's place.";

#[derive(Debug, Clone, PartialEq)]
pub enum Failure {
    Class(ErrorClass, String),
    Blocked(String),
}

fn failed(language: domain::Language, kind: Option<FailureKind>, message: String) -> Failure {
    match kind {
        Some(FailureKind::UsageLimit { .. }) => Failure::Blocked(crate::texts::usage_limit(language, &message)),
        Some(FailureKind::Unavailable) => Failure::Class(ErrorClass::Transient, message),
        _ => Failure::permanent(message),
    }
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
}

impl Profile {
    fn parse(text: &str) -> Profile {
        let header = agent_engine::tools::agent_file::header(text);
        let tools = header["tools"]
            .as_array()
            .map(|list| {
                list.iter()
                    .filter_map(|tool| tool.as_str())
                    .map(str::trim)
                    .filter(|tool| !tool.is_empty())
                    .map(str::to_owned)
                    .collect()
            })
            .unwrap_or_default();
        Profile {
            mode: header["permissionMode"].as_str().and_then(domain::command::mode_named),
            model: header["model"].as_str().filter(|model| !model.is_empty()).map(str::to_owned),
            tools,
        }
    }
}

pub async fn blocking<T: Send + 'static>(work: impl FnOnce() -> T + Send + 'static) -> Result<T, Failure> {
    tokio::task::spawn_blocking(work).await.map_err(|error| Failure::Class(ErrorClass::Transient, error.to_string()))
}

pub async fn worktree(
    shared: &Shared,
    checkout: Checkout,
    branch: &str,
    base: Option<String>,
) -> Result<Worktree, Failure> {
    if checkout.read_only {
        return consulted(shared, checkout, branch).await;
    }
    let (data, branch) = (shared.config.data.clone(), branch.to_owned());
    let folder = repos::folder(&branch, &checkout.id);
    let (repo, named) = (checkout.directory.clone(), branch.clone());
    let path = blocking(move || git::worktree(repo.as_deref(), &data, &named, base.as_deref(), &folder))
        .await?
        .map_err(|error| match git::missing_base(&error) {
            Some(base) => Failure::Blocked(crate::bases::blocked(&checkout.id, base)),
            None => Failure::Class(ErrorClass::Transient, format!("the worktree could not be made: {error}")),
        })?;
    Ok(Worktree { checkout, path, branch })
}

async fn consulted(shared: &Shared, checkout: Checkout, branch: &str) -> Result<Worktree, Failure> {
    let folder = checkout.directory.clone().filter(|folder| folder.is_dir()).ok_or_else(|| {
        Failure::permanent(format!("the read-only repository {} has no folder on this machine", checkout.name))
    })?;
    let (data, named, repo) = (shared.config.data.clone(), repos::folder(branch, &checkout.id), folder.clone());
    let copy = blocking(move || git::detached(&repo, &data, &named)).await?.map_err(|error| {
        Failure::Class(ErrorClass::Transient, format!("the read-only copy could not be made: {error}"))
    })?;
    Ok(Worktree { checkout, path: copy.unwrap_or(folder), branch: String::new() })
}

pub async fn worktrees(shared: &Shared, item: &ItemId, wanted: Option<&str>) -> Result<Vec<Worktree>, Failure> {
    let state = shared.item(item).map_err(|error| Failure::permanent(error.to_string()))?;
    let checkouts = shared.checkouts(&state);
    let usual = checkouts.first().map_or_else(|| git::head_of(&state, ""), |first| git::head_of(&state, &first.id));
    let chosen = wanted.filter(|name| *name != usual);
    let mut made = Vec::new();
    for checkout in checkouts {
        let branch = chosen.map_or_else(|| git::head_of(&state, &checkout.id), str::to_owned);
        let base = git::base_of(&state, &checkout.id);
        made.push(worktree(shared, checkout, &branch, base).await?);
    }
    made.sort_by_key(|tree| tree.checkout.read_only);
    Ok(made)
}

async fn heads(worktrees: &[Worktree]) -> Result<Vec<Option<String>>, Failure> {
    let places: Vec<PathBuf> = worktrees.iter().map(|worktree| worktree.path.clone()).collect();
    blocking(move || places.iter().map(|place| git::head(place)).collect()).await
}

pub fn in_place(command: &Command) -> bool {
    command.args["inPlace"].as_bool() == Some(true)
}

fn folders(shared: &Shared, item: &ItemId) -> Result<Vec<Worktree>, Failure> {
    let state = shared.item(item).map_err(|error| Failure::permanent(error.to_string()))?;
    let found = shared.checkouts(&state).into_iter().filter_map(|checkout| {
        let path = checkout.directory.clone().filter(|folder| folder.is_dir())?;
        Some(Worktree { checkout, path, branch: String::new() })
    });
    Ok(found.collect())
}

pub fn branch_of(command: &Command) -> Option<&str> {
    command.args["worktree"].as_str().filter(|name| !name.is_empty())
}

fn model(shared: &Shared, wanted: Option<&str>) -> ModelOption {
    let models = shared.config.engine.models();
    wanted
        .and_then(|wanted| models.iter().find(|option| option.id == wanted))
        .cloned()
        .or_else(|| models.first().cloned())
        .unwrap_or_else(|| ModelOption::unlisted(wanted.unwrap_or("unknown")))
}

pub(crate) fn language_of(shared: &Shared, input: &Value) -> domain::Language {
    input["language"]
        .as_str()
        .and_then(domain::Language::named)
        .or_else(|| shared.store.language().ok())
        .unwrap_or_default()
}

fn prompt_of(input: &Value) -> String {
    match input.get("prompt").and_then(Value::as_str) {
        Some(prompt) => prompt.to_owned(),
        None => input.to_string(),
    }
}

pub(crate) fn images_of(input: &Value) -> Vec<domain::Image> {
    let listed = input.get("images").and_then(Value::as_array).map(Vec::as_slice).unwrap_or_default();
    listed
        .iter()
        .filter_map(|image| domain::Image::from_mcp(image["media_type"].as_str()?, image["data"].as_str()?).ok())
        .collect()
}

fn request(shared: &Shared, profile: &Profile, command: &Command, cwd: &Path, resume: Option<&str>) -> PromptRequest {
    let input = &command.args["input"];
    let wanted = command.args["model"].as_str().or(profile.model.as_deref());
    let tiers = agent_engine::tiers::current();
    let tier = wanted.and_then(|named| tiers.find(named)).map(|spec| spec.name.clone());
    let option = model(shared, wanted.filter(|_| tier.is_none()));
    let carried = input.get("session").and_then(Value::as_str);
    let text = if resume.is_some() { RESUME.to_owned() } else { prompt_of(input) };
    let mut request = PromptRequest::plain(cwd.to_path_buf(), text, option);
    request.session.tier = tier;
    if resume.is_none() {
        request.prompt.images = images_of(input);
    }
    if let Some(effort) = command.args["effort"].as_str().and_then(domain::Effort::named) {
        request.session.effort = effort;
    }
    request.session.resume = resume.or(carried).map(|id| SessionId(id.to_owned()));
    request.session.output_style = shared.store.output_style();
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
    plan: Option<String>,
    failure: Option<FailureKind>,
    planning: bool,
    plan_ask: Option<AskId>,
    approved: bool,
}

impl Collected {
    fn absorb(&mut self, later: Collected) {
        if !later.text.is_empty() {
            if !self.text.is_empty() {
                self.text.push_str("\n\n");
            }
            self.text.push_str(&later.text);
        }
        if later.session.is_some() {
            self.session = later.session;
        }
        if !later.output.is_null() {
            self.output = later.output;
        }
        self.checkpoint = self.checkpoint.or(later.checkpoint);
        if later.plan.is_some() {
            self.plan = later.plan;
        }
        self.approved |= later.approved;
    }
}

struct Client(u64, Arc<Shared>);

impl Drop for Client {
    fn drop(&mut self) {
        self.1.steering.disown(self.0);
        clients::release(self.0);
    }
}

const WAKE_POLL: Duration = Duration::from_secs(1);
const TEAMMATE_POLL: Duration = Duration::from_millis(500);

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
        let state = self.shared.item(self.item).map_err(|error| Failure::permanent(error.to_string()))?;
        let here = in_place(self.command);
        let state = if here { state } else { crate::chores::ensure_branch(self.shared, &state, &self.key.to_string()) };
        let resources = self.shared.resources_in(self.shared.home(&state.input).as_deref());
        let definition =
            resources.agent(&name).ok_or_else(|| Failure::permanent(format!("there is no agent named {name}")))?;
        let profile = Profile::parse(&definition.text);
        let trees = if here {
            folders(self.shared, self.item)?
        } else {
            worktrees(self.shared, self.item, branch_of(self.command)).await?
        };
        let first = trees.first().ok_or_else(|| Failure::permanent("the item has no repository"))?;
        let cwd = first.path.clone();
        let bases = heads(&trees).await?;
        let cloud = crate::cloud::lease(self.shared, self.command, &state.input).await?;
        let mut request = request(self.shared, &profile, self.command, &cwd, self.resume.as_deref());
        request.session.language = Some(language_of(self.shared, &state.input));
        request.prompt.shown = Some(domain::request::Shown {
            text: request.prompt.text.clone(),
            attached: Vec::new(),
            from: Some(if self.key.step.is_empty() { state.workflow.clone() } else { self.key.step.clone() }),
        });
        request.session.agent = Some(MainAgentSpec { name: name.clone(), definition: Some(definition.text.clone()) });
        let client = Client(clients::fresh(), Arc::clone(self.shared));
        request.host.credentials.client = client.0;
        if let Some(application) = crate::application::id_of(&state.input) {
            crate::application::offer(client.0, &self.shared.store, application);
        }
        let hub = &self.shared.config.connectors;
        request.session.requires =
            crate::data_policy::requires(&self.shared.store, hub, &state.input, &self.command.args);
        let mut environment = crate::application::environment(&self.shared.store, &state.input);
        environment.extend(cloud.as_ref().map(|lease| lease.environment.clone()).unwrap_or_default());
        request.host.environment = environment;
        request.host.owner = Some(crate::owners::of(&self.shared.store, &state));
        let (consulted, writable): (Vec<&Worktree>, Vec<&Worktree>) =
            trees.iter().skip(1).partition(|tree| tree.checkout.read_only);
        request.session.add_dirs = writable.into_iter().map(|tree| tree.path.clone()).collect();
        request.session.read_dirs = consulted.into_iter().map(|tree| tree.path.clone()).collect();
        if first.checkout.read_only {
            request.session.read_dirs.insert(0, cwd.clone());
        }
        if let Some(text) = repos::described(&trees) {
            let system = request.session.append_system.take().unwrap_or_default();
            request.session.append_system = Some(if system.is_empty() { text } else { format!("{system}\n\n{text}") });
        }
        self.note(json!({ "worktree": cwd.to_string_lossy() }));
        let collected = self.drive(&request).await?;
        let pushed: Vec<(PathBuf, String)> = trees
            .iter()
            .filter(|tree| !here && !tree.checkout.read_only)
            .map(|tree| (tree.path.clone(), tree.branch.clone()))
            .collect();
        blocking(move || {
            for (place, branch) in &pushed {
                git::push(place, branch);
            }
        })
        .await?;
        if here {
            return Ok(json!({
                "text": collected.text, "session": collected.session, "output": collected.output, "plan": collected.plan,
                "checkpoint": collected.checkpoint, "agent": name, "inPlace": true, "folder": cwd.to_string_lossy(),
            }));
        }
        let commits = heads(&trees).await?;
        let summaries: Vec<Value> = trees
            .iter()
            .zip(bases.iter().zip(&commits))
            .map(|(tree, (base, commit))| repos::summary(tree, base.as_deref(), commit.as_deref()))
            .collect();
        Ok(json!({
            "text": collected.text, "session": collected.session, "output": collected.output, "plan": collected.plan,
            "checkpoint": collected.checkpoint, "base": bases.first().cloned().flatten(),
            "commit": commits.first().cloned().flatten(), "branch": first.branch, "agent": name,
            "worktree": cwd.to_string_lossy(), "repositories": summaries,
        }))
    }

    async fn drive(&self, request: &PromptRequest) -> Result<Collected, Failure> {
        let mut collected = self.turn(request).await?;
        let mut next = request.clone();
        while !collected.approved
            && let Some(prompt) = self.woken(request.host.credentials.client, &collected).await
        {
            next.prompt = Prompt::typed(prompt);
            next.session.resume = collected.session.clone().map(SessionId).or(next.session.resume);
            let later = self.turn(&next).await?;
            collected.absorb(later);
        }
        Ok(collected)
    }

    async fn turn(&self, request: &PromptRequest) -> Result<Collected, Failure> {
        let (sender, receiver) = mpsc::unbounded_channel();
        let sink: Arc<dyn EventSink> = Arc::new(move |_run: RunId, event: EngineEvent| {
            let _ = sender.send(event);
        });
        let started = self.shared.config.engine.start(RunId::new(), request, sink);
        let handle = started.map_err(|error| Failure::permanent(error.to_string()))?;
        let mut running = Running { run: Arc::new(Mutex::new(handle)), done: false };
        self.reopen(&running.run);
        let planning = request.policy.permission == PermissionMode::Plan;
        let followed = self.follow(receiver, &running.run, request.host.credentials.client, planning).await;
        self.shared.steering.release(&running.run);
        self.close_asks();
        let collected = followed?;
        running.done = true;
        Ok(collected)
    }

    fn ask_limit(&self) -> Duration {
        self.command.args["askTimeout"].as_u64().map_or(self.shared.config.limits.ask_timeout, Duration::from_millis)
    }

    async fn follow(
        &self,
        mut receiver: mpsc::UnboundedReceiver<EngineEvent>,
        run: &SharedRun,
        client: u64,
        planning: bool,
    ) -> Result<Collected, Failure> {
        let mut collected = Collected { planning, ..Collected::default() };
        let mut impeded = false;
        let mut teammates = tokio::time::interval(TEAMMATE_POLL);
        loop {
            let deadline = self.ask_deadline(impeded);
            let event = tokio::select! {
                event = receiver.recv() => event,
                _ = teammates.tick() => {
                    self.teammates_asked(client, &collected);
                    continue;
                }
                () = sleep_for(deadline) => {
                    impeded = true;
                    self.impede(true);
                    continue;
                }
            };
            let Some(event) = event else {
                return Err(Failure::Class(ErrorClass::Transient, "the agent run ended without finishing".into()));
            };
            if let EngineEvent::SessionStarted(session) = &event {
                self.shared.steering.own(&session.0, client);
            }
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
        let language = crate::texts::of_item(self.shared, self.item);
        let reason = on.then(|| crate::texts::waiting_answer(language).to_owned());
        let current = self.shared.item(self.item).ok().and_then(|item| item.impediment);
        let ours = current.as_ref().is_some_and(|found| crate::texts::is_waiting_answer(found.reason.as_deref()));
        if on == ours || (!on && current.is_none()) {
            return;
        }
        let _ = self.shared.append(self.item, &[WorkflowEvent::ImpedimentSet { on, reason }]);
    }

    fn take(&self, event: EngineEvent, collected: &mut Collected, run: &SharedRun) -> Option<Result<(), Failure>> {
        self.watched(&event, collected);
        match event {
            EngineEvent::SessionStarted(session) => {
                self.shared.steering.follow(&session.0, run);
                self.started(session.0, collected);
            }
            EngineEvent::Checkpoint(index) => {
                collected.checkpoint.get_or_insert(index);
            }
            EngineEvent::TextDelta(piece) => collected.text.push_str(&piece),
            EngineEvent::StructuredOutput(value) => collected.output = value,
            EngineEvent::Asked(ask) => {
                if let AskKind::Plan { plan } = &ask.kind {
                    collected.plan = Some(plan.clone());
                    collected.plan_ask = Some(ask.id.clone());
                }
                self.asked(ask, collected, run);
            }
            EngineEvent::AskWithdrawn(ask) => self.closed(ask),
            EngineEvent::Answered { ask, decision } => {
                let plan_approved = collected.planning
                    && collected.plan_ask.as_ref() == Some(&ask)
                    && matches!(decision, Decision::Approve(_));
                self.closed(ask);
                if plan_approved {
                    collected.approved = true;
                    run.lock().unwrap_or_else(PoisonError::into_inner).cancel();
                    return Some(Ok(()));
                }
            }
            EngineEvent::Completed { .. } => return Some(Ok(())),
            EngineEvent::Failed(message) => {
                return Some(Err(failed(crate::texts::of_item(self.shared, self.item), collected.failure, message)));
            }
            EngineEvent::Failure(kind) => collected.failure = Some(kind),
            EngineEvent::Cancelled => return Some(Err(Failure::permanent("the agent run was cancelled"))),
            _ => {}
        }
        None
    }

    fn closed(&self, ask: AskId) {
        if self.shared.asks.close(&ask).is_some() {
            self.note(json!({ "asks": { ask.0.clone(): null } }));
            self.shared.emit(OrchestratorEvent::AskClosed { item: self.item.clone(), ask });
        }
    }

    fn watched(&self, event: &EngineEvent, collected: &Collected) {
        let Some(watch) = self.shared.config.watch.get() else { return };
        let session = match event {
            EngineEvent::SessionStarted(started) => Some(started.0.as_str()),
            _ => collected.session.as_deref(),
        };
        if let Some(session) = session {
            watch(session, event);
        }
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

    async fn woken(&self, client: u64, collected: &Collected) -> Option<String> {
        loop {
            self.teammates_asked(client, collected);
            match clients::waiting(client, schedules::now()) {
                Waiting::Idle => return None,
                Waiting::Due(prompt) => return Some(prompt),
                Waiting::Later(_) => tokio::time::sleep(WAKE_POLL).await,
            }
        }
    }

    fn teammates_asked(&self, client: u64, collected: &Collected) {
        for asked in agent_engine::team::lead_inbox::take_asks(client) {
            let kind = match &asked.asked {
                domain::team::Asked::Permission { tool_name, description, rememberable } => {
                    AskKind::permission(tool_name.clone(), description.clone(), *rememberable)
                }
                domain::team::Asked::Plan { plan } => AskKind::Plan { plan: plan.clone() },
            };
            let id = domain::AskId(format!("teammate-{}-{}", asked.teammate, asked.request_id));
            let pending = PendingAsk {
                item: self.item.clone(),
                step: self.key.step.clone(),
                session: collected.session.clone(),
                ask: AgentAsk { id: id.clone(), kind },
                since: now_ms(),
            };
            if !self.shared.asks.open_teammate(pending.clone(), self.key.clone(), client, asked) {
                continue;
            }
            self.note(json!({ "asks": { id.0.clone(): serde_json::to_value(&pending).unwrap_or(Value::Null) } }));
            self.announce(pending);
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
        crate::chores::recap(self.shared, self.item, &ask.id.0);
    }
}

async fn sleep_for(duration: Option<Duration>) {
    match duration {
        Some(duration) => tokio::time::sleep(duration).await,
        None => std::future::pending().await,
    }
}

pub async fn rewind_session(shared: &Shared, cwd: PathBuf, session: &str, checkpoint: u32) -> bool {
    let option = model(shared, None);
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
