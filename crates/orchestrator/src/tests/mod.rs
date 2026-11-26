mod application;
mod cloud;
mod control;
mod factory;
mod flow;
mod links;
mod package;
mod repos;
mod restart;
mod triggers;

use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use agent_engine::{AgentEngine, EngineError, EventSink, RunHandle};
use async_trait::async_trait;
use connectors::descriptor::object;
use connectors::oauth::Dialect;
use connectors::{
    AccountOption, ActionError, ActionSpec, AuthSpec, Connectors, Credentials, DeviceApp, FakeHttp, FixedClock, Grant,
    Http, MemoryTokenStore, Provider, ProviderDescriptor, TokenKeeper, TokenStore,
};
use domain::connection::{Capability, ConnectionId, ProviderKey};
use domain::workflow::{ItemId, WorkItem};
use domain::{AgentAsk, AskId, AskKind, Decision, EngineEvent, ModelOption, PromptRequest, RunId, SessionId};
use serde_json::{Value, json};
use store::{ConnectionRecord, Store};

use crate::{Config, Handle, Limits, Orchestrator, Resources, Timeouts, git};

pub enum Reply {
    Answer(Value),
    Fail(ActionError),
    Hang,
}

#[derive(Debug, Default)]
pub struct FakeProvider {
    replies: Mutex<HashMap<String, VecDeque<String>>>,
    errors: Mutex<HashMap<String, VecDeque<ActionError>>>,
    hangs: Mutex<HashMap<String, usize>>,
    answers: Mutex<HashMap<String, Value>>,
    pub calls: Mutex<Vec<(String, Value, String)>>,
}

const ACTIONS: [&str; 16] = [
    "tracker.issue",
    "tracker.create_issue",
    "tracker.update_issue",
    "tracker.link_issues",
    "code.open_pull_request",
    "pipelines.runs",
    "changes.create",
    "changes.update",
    "changes.get",
    "tracker.comment",
    "tracker.transition",
    "tracker.set_impediment",
    "pipelines.run",
    "code.merge",
    "cloud.identity",
    "design.frame",
];

impl FakeProvider {
    pub fn leaked() -> &'static FakeProvider {
        Box::leak(Box::new(FakeProvider::default()))
    }

    pub fn then(&self, action: &str, reply: Reply) {
        match reply {
            Reply::Answer(value) => {
                self.replies.lock().expect("lock").entry(action.into()).or_default().push_back(value.to_string());
            }
            Reply::Fail(error) => self.errors.lock().expect("lock").entry(action.into()).or_default().push_back(error),
            Reply::Hang => *self.hangs.lock().expect("lock").entry(action.into()).or_default() += 1,
        }
    }

    pub fn always(&self, action: &str, value: Value) {
        self.answers.lock().expect("lock").insert(action.into(), value);
    }

    pub fn count(&self, action: &str) -> usize {
        self.calls.lock().expect("lock").iter().filter(|(name, _, _)| name == action).count()
    }

    pub fn keys(&self, action: &str) -> Vec<String> {
        let calls = self.calls.lock().expect("lock");
        calls.iter().filter(|(name, _, _)| name == action).map(|(_, _, key)| key.clone()).collect()
    }
}

#[async_trait]
impl Provider for FakeProvider {
    fn descriptor(&self) -> ProviderDescriptor {
        ProviderDescriptor {
            key: ProviderKey::from("fake"),
            name: "Fake".into(),
            logo: "<svg/>".into(),
            capabilities: vec![
                Capability::from("tracker"),
                Capability::from("pipelines"),
                Capability::from("code"),
                Capability::from("cloud"),
                Capability::from("changes"),
                Capability::from("design"),
            ],
            auth: AuthSpec::Device(DeviceApp {
                client_id: "c".into(),
                client_secret: None,
                device_endpoint: "https://login.test/device".into(),
                token_endpoint: "https://login.test/token".into(),
                scopes: Vec::new(),
                start_fields: Vec::new(),
                dialect: Dialect::Form,
            }),
            actions: ACTIONS.iter().map(|name| ActionSpec::read(name, object(&[], &[]), json!({}))).collect(),
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
        let count = {
            let mut calls = self.calls.lock().expect("lock");
            calls.push((action.to_owned(), args.clone(), key.to_owned()));
            calls.iter().filter(|(name, _, _)| name == action).count()
        };
        let hang = self.hangs.lock().expect("lock").get_mut(action).is_some_and(|left| {
            let hanging = *left > 0;
            *left = left.saturating_sub(1);
            hanging
        });
        if hang {
            tokio::time::sleep(Duration::from_secs(3600)).await;
        }
        if let Some(error) = self.errors.lock().expect("lock").get_mut(action).and_then(VecDeque::pop_front) {
            return Err(error);
        }
        if let Some(text) = self.replies.lock().expect("lock").get_mut(action).and_then(VecDeque::pop_front) {
            return Ok(serde_json::from_str(&text).expect("json"));
        }
        Ok(self.answers.lock().expect("lock").get(action).cloned().unwrap_or_else(|| json!({ "n": count, "key": key })))
    }

    async fn accounts(&self, _http: &dyn Http, _creds: &Credentials) -> Result<Vec<AccountOption>, ActionError> {
        Ok(Vec::new())
    }
}

#[derive(Debug, Default)]
pub struct EngineState {
    pub requests: Mutex<Vec<PromptRequest>>,
    pub hold: AtomicBool,
    pub ask: AtomicBool,
    pub keeps_going: AtomicBool,
    pub limited: AtomicBool,
    pub write: Mutex<Option<String>>,
    pub answers: Mutex<Vec<(AskId, Decision)>>,
}

#[derive(Debug, Clone, Default)]
pub struct FakeEngine {
    pub state: Arc<EngineState>,
}

impl FakeEngine {
    pub fn requests(&self) -> Vec<PromptRequest> {
        let all = self.state.requests.lock().expect("lock").clone();
        all.into_iter()
            .filter(|request| !request.session.command.is_some_and(domain::SessionCommand::is_chore))
            .collect()
    }

    pub fn chores(&self, command: domain::SessionCommand) -> Vec<PromptRequest> {
        let all = self.state.requests.lock().expect("lock").clone();
        all.into_iter().filter(|request| request.session.command == Some(command)).collect()
    }

    pub fn titled(&self) -> Vec<PromptRequest> {
        let all = self.state.requests.lock().expect("lock").clone();
        all.into_iter().filter(|request| request.session.command == Some(domain::SessionCommand::Title)).collect()
    }

    pub fn runs(&self) -> usize {
        self.requests().iter().filter(|request| request.session.command.is_none()).count()
    }
}

struct FakeRun {
    cancelled: Arc<AtomicBool>,
    answer: std::sync::mpsc::Sender<Decision>,
    state: Arc<EngineState>,
}

impl RunHandle for FakeRun {
    fn cancel(&mut self) {
        self.cancelled.store(true, Ordering::SeqCst);
    }

    fn answer(&mut self, ask: &AskId, decision: &Decision) {
        self.state.answers.lock().expect("lock").push((ask.clone(), decision.clone()));
        let _ = self.answer.send(decision.clone());
    }
}

fn commit_file(cwd: &Path, name: &str) {
    std::fs::write(cwd.join(name), "mudou").expect("write");
    for args in [vec!["add", "-A"], vec!["commit", "-qm", "agente"]] {
        let _ = platform::git::command(cwd).args(["-c", "user.email=a@b", "-c", "user.name=a"]).args(args).output();
    }
}

impl AgentEngine for FakeEngine {
    fn models(&self) -> Vec<ModelOption> {
        Vec::new()
    }

    fn start(
        &self,
        _run: RunId,
        request: &PromptRequest,
        sink: Arc<dyn EventSink>,
    ) -> Result<Box<dyn RunHandle>, EngineError> {
        let number = {
            let mut requests = self.state.requests.lock().expect("lock");
            requests.push(request.clone());
            let chore = |request: &PromptRequest| request.session.command.is_some_and(domain::SessionCommand::is_chore);
            requests.iter().filter(|request| !chore(request)).count()
        };
        let cancelled = Arc::new(AtomicBool::new(false));
        let (answer, answers) = std::sync::mpsc::channel();
        let (state, request, stop) = (Arc::clone(&self.state), request.clone(), Arc::clone(&cancelled));
        std::thread::spawn(move || {
            let run = RunId::new();
            let session = request.session.resume.clone().unwrap_or_else(|| SessionId(format!("s{number}")));
            sink.emit(run, EngineEvent::SessionStarted(session));
            sink.emit(run, EngineEvent::Checkpoint(u32::try_from(number).unwrap_or(0)));
            if request.session.command.is_some() {
                let answer = match request.session.command {
                    Some(domain::SessionCommand::Title) => Some(domain::ChoreAnswer::Title("Loja virtual".into())),
                    Some(domain::SessionCommand::Recap) => {
                        Some(domain::ChoreAnswer::Recap("Plano pronto; falta aprovar.".into()))
                    }
                    Some(domain::SessionCommand::HeadsUp) => Some(domain::ChoreAnswer::HeadsUp(domain::HeadsUp {
                        urgent: false,
                        learn: "migração".into(),
                        title: "A migração apaga a coluna antiga".into(),
                        explain: "Rode o backup antes do deploy.".into(),
                    })),
                    _ => None,
                };
                if let Some(answer) = answer {
                    sink.emit(run, EngineEvent::Chore(answer));
                }
                sink.emit(run, EngineEvent::Completed { duration_ms: 0, cost_usd: None });
                return;
            }
            let held = state.hold.load(Ordering::SeqCst) && request.prompt.text != crate::agent::RESUME;
            if let Some(name) = state.write.lock().expect("lock").clone() {
                commit_file(&request.session.cwd, &name);
            }
            while held && state.hold.load(Ordering::SeqCst) && !stop.load(Ordering::SeqCst) {
                std::thread::sleep(Duration::from_millis(10));
            }
            if state.ask.load(Ordering::SeqCst) {
                let ask =
                    AgentAsk { id: AskId(format!("ask-{number}")), kind: AskKind::Plan { plan: "o plano".into() } };
                let id = ask.id.clone();
                sink.emit(run, EngineEvent::Asked(ask));
                let mut decision = None;
                while !stop.load(Ordering::SeqCst) && decision.is_none() {
                    decision = answers.recv_timeout(Duration::from_millis(10)).ok();
                }
                if let Some(decision) = decision.filter(|_| state.keeps_going.load(Ordering::SeqCst)) {
                    sink.emit(run, EngineEvent::Answered { ask: id, decision });
                    while !stop.load(Ordering::SeqCst) {
                        std::thread::sleep(Duration::from_millis(10));
                    }
                    return;
                }
            }
            if stop.load(Ordering::SeqCst) {
                return;
            }
            if state.limited.load(Ordering::SeqCst) {
                sink.emit(run, EngineEvent::Failure(domain::FailureKind::UsageLimit { resets_at: None }));
                sink.emit(run, EngineEvent::Failed("HTTP 400: You have reached the limit.".into()));
                return;
            }
            if request.prompt.text.contains("a equipe pede permissão") {
                let team = agent_engine::team::Team::named(&request.session.cwd.join(".equipe"), "t");
                let _ = team.open("s");
                let _ = team.join("tester", domain::team::Member::default());
                agent_engine::team::lead_inbox::watch(&team, request.host.credentials.client);
                let asked = domain::team::Protocol::PermissionRequest {
                    request_id: "r1".into(),
                    from: "tester".into(),
                    tool_name: "Bash".into(),
                    description: "rodar a migração".into(),
                    rememberable: false,
                };
                let _ = team.send(domain::team::LEAD, "tester", &asked.text(), None);
            }
            if request.prompt.text.contains("chame a equipe") {
                let mail = domain::inbox::Notice::new("teammate tester", "terminei a parte da equipe");
                agent_engine::tools::inbox::push(request.host.credentials.client, mail);
            }
            sink.emit(run, EngineEvent::TextDelta(format!("feito {number}")));
            sink.emit(run, EngineEvent::StructuredOutput(json!({ "run": number })));
            sink.emit(run, EngineEvent::Completed { duration_ms: 0, cost_usd: None });
        });
        Ok(Box::new(FakeRun { cancelled, answer, state: Arc::clone(&self.state) }))
    }
}

pub struct World {
    pub dir: tempfile::TempDir,
    pub engine: FakeEngine,
    pub provider: &'static FakeProvider,
    pub git: bool,
    pub parallel: usize,
    pub sync: bool,
    pub cloud: bool,
}

pub const SERVER: &str = "http://127.0.0.1:9";

pub const WORKER: &str =
    "---\nname: worker\npermissionMode: auto\nmodel: auto\ntools: [Read, Edit]\n---\nVocê trabalha.";

impl World {
    pub fn new() -> World {
        let world = World {
            dir: tempfile::tempdir().expect("dir"),
            engine: FakeEngine::default(),
            provider: FakeProvider::leaked(),
            git: false,
            parallel: 4,
            sync: false,
            cloud: false,
        };
        world.write("agents/worker.md", WORKER);
        world
    }

    pub fn repo(&self) -> PathBuf {
        self.dir.path().join("repo")
    }

    pub fn write(&self, path: &str, text: &str) {
        let file = self.repo().join(brand::DIR).join(path);
        std::fs::create_dir_all(file.parent().expect("parent")).expect("dir");
        std::fs::write(file, text).expect("write");
    }

    pub fn workflow(&self, name: &str, body: &str) {
        self.write(&format!("workflows/{name}.ts"), body);
    }

    pub fn init_git(&mut self) {
        let repo = self.repo();
        std::fs::write(repo.join("README"), "oi").expect("write");
        for args in [vec!["init", "-q", "-b", "main"], vec!["add", "-A"], vec!["commit", "-qm", "início"]] {
            let done =
                platform::git::command(&repo).args(["-c", "user.email=a@b", "-c", "user.name=a"]).args(args).output();
            assert!(done.is_ok_and(|done| done.status.success()));
        }
        self.git = true;
    }

    pub fn store(&self) -> Arc<Store> {
        Arc::new(Store::open(&self.dir.path().join("app.db"), 0).expect("store"))
    }

    pub async fn start(&self) -> Handle {
        let store = self.store();
        let record = ConnectionRecord {
            id: "c1".into(),
            provider: "fake".into(),
            label: "Fake".into(),
            account: Some("me".into()),
            capabilities: vec!["tracker".into(), "pipelines".into(), "code".into(), "changes".into(), "design".into()],
            settings: json!({}),
        };
        store.save_connection(&record).expect("connection");
        if self.cloud {
            let cloud = ConnectionRecord { id: "aws".into(), capabilities: vec!["cloud".into()], ..record };
            store.save_connection(&cloud).expect("cloud connection");
        }
        let tokens = Arc::new(MemoryTokenStore::new());
        for id in ["c1", "aws"] {
            tokens.save(&ConnectionId::from(id), &Grant::bearer("tok")).await.expect("grant");
        }
        let keeper = TokenKeeper::new(tokens, Arc::new(FakeHttp::new()), Arc::new(FixedClock::at_second(1_000)));
        let provider: &'static dyn Provider = self.provider;
        let hub = Arc::new(Connectors::with_providers(&[provider], Arc::new(FakeHttp::new()), keeper));
        let listed = content::builtin_files("").into_iter().map(|(path, _)| path.to_owned()).collect();
        let resources =
            Resources::new(Some(self.repo().join(brand::DIR)), None, Arc::new(content::builtin_text)).listing(listed);
        let mut config = Config::new(store, hub, Arc::new(self.engine.clone()), self.dir.path().join("data"))
            .with_resources(resources)
            .with_limits(Limits { max_parallel_items: self.parallel, ..limits() })
            .with_server(SERVER)
            .with_schedules(crate::Schedules { enabled: false, ..crate::Schedules::from_config() });
        if self.git {
            config.repo = Some(self.repo());
        }
        if !self.sync {
            config = config.without_sync();
        }
        Orchestrator::start(config).expect("start")
    }
}

pub fn limits() -> Limits {
    Limits {
        max_parallel_items: 4,
        retry: domain::effect::RetryPolicy {
            max: 4,
            initial: Duration::from_millis(5),
            max_interval: Duration::from_millis(20),
            multiplier: 2.0,
            jitter: 0.1,
        },
        timeouts: Timeouts {
            agent: Duration::from_secs(60),
            exec: Duration::from_secs(60),
            call: Duration::from_secs(60),
        },
        ask_timeout: Duration::from_secs(60),
    }
}

pub async fn until(handle: &Handle, id: &ItemId, check: impl Fn(&WorkItem) -> bool) -> WorkItem {
    for _ in 0..500 {
        if let Ok(Some(item)) = handle.item(id)
            && check(&item)
        {
            return item;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("the item never got there: {:?}", handle.item(id))
}

pub async fn ended(handle: &Handle, id: &ItemId) -> WorkItem {
    until(handle, id, WorkItem::is_ended).await
}

pub async fn crash(handle: Handle) {
    drop(handle);
    tokio::time::sleep(Duration::from_millis(150)).await;
}

pub fn output(item: &WorkItem) -> Value {
    match &item.ending {
        Some(domain::workflow::Ending::Finished { output }) => output.clone(),
        other => panic!("the item did not finish: {other:?}"),
    }
}
