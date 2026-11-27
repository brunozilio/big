#![allow(clippy::expect_used, clippy::panic, dead_code, reason = "a failed setup step stops the test")]

pub mod agents;
pub mod github;
pub mod tracker;

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use connectors::{Connectors, FixedClock, Grant, MemoryTokenStore, Provider, TokenKeeper, TokenStore};
use domain::connection::ConnectionId;
use http_body_util::BodyExt;
use orchestrator::{Config, Handle, Limits, Orchestrator, Resources, Timeouts};
use serde_json::{Value, json};
use store::{ConnectionRecord, Store};
use tower::ServiceExt;

use agents::Agents;
use github::Github;
use tracker::Tracker;

pub const CODE: &str = "gh";

pub struct Hub {
    pub dir: tempfile::TempDir,
    pub github: Arc<Github>,
    pub tracker: &'static Tracker,
    pub agents: Agents,
}

pub struct Server {
    pub handle: Handle,
    pub router: Router,
    pub store: Arc<Store>,
}

fn git(dir: &Path, args: &[&str]) {
    let done = std::process::Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["-c", "user.email=a@b", "-c", "user.name=a"])
        .args(args)
        .output()
        .expect("git");
    assert!(done.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&done.stderr));
}

fn limits() -> Limits {
    Limits {
        max_parallel_items: 4,
        retry: domain::effect::RetryPolicy {
            max: 3,
            initial: Duration::from_millis(5),
            max_interval: Duration::from_millis(20),
            multiplier: 2.0,
            jitter: 0.1,
        },
        timeouts: Timeouts {
            agent: Duration::from_secs(120),
            exec: Duration::from_secs(60),
            call: Duration::from_secs(30),
        },
        ask_timeout: Duration::from_secs(120),
    }
}

fn sync_config() -> sync::SyncConfig {
    sync::SyncConfig {
        fast_poll: Duration::from_millis(100),
        slow_poll: Duration::from_millis(200),
        reconcile_every: Duration::from_millis(300),
        seen_retention: Duration::from_secs(3600),
        ended_retention: None,
        retry: domain::effect::RetryPolicy {
            max: u32::MAX,
            initial: Duration::from_millis(50),
            max_interval: Duration::from_millis(200),
            ..domain::effect::RetryPolicy::default()
        },
    }
}

impl Hub {
    pub fn new() -> Hub {
        let dir = tempfile::tempdir().expect("dir");
        let repo = dir.path().join("repo");
        std::fs::create_dir_all(&repo).expect("repo");
        std::fs::write(repo.join("README.md"), "oi\n").expect("readme");
        git(&repo, &["init", "-q", "-b", "main"]);
        git(&repo, &["add", "-A"]);
        git(&repo, &["commit", "-qm", "início"]);
        git(&repo, &["branch", "develop"]);
        std::fs::create_dir_all(dir.path().join("data")).expect("data");
        let github = Arc::new(Github::new(repo));
        Hub { dir, github, tracker: Tracker::leaked(), agents: Agents::default() }
    }

    pub fn repo(&self) -> PathBuf {
        self.dir.path().join("repo")
    }

    pub fn data(&self) -> PathBuf {
        self.dir.path().join("data")
    }

    pub fn write(&self, path: &str, text: &str) {
        let file = self.repo().join(brand::DIR).join(path);
        std::fs::create_dir_all(file.parent().expect("parent")).expect("dir");
        std::fs::write(file, text).expect("write");
    }

    fn connections(store: &Store) {
        let code = ConnectionRecord {
            id: CODE.into(),
            provider: "github".into(),
            label: "Código".into(),
            account: Some("agente".into()),
            capabilities: vec!["code".into(), "pipelines".into()],
            settings: json!({ "api": github::API, "repo": github::REPO }),
        };
        let issues = ConnectionRecord {
            id: tracker::CONNECTION.into(),
            provider: tracker::KEY.into(),
            label: "Tarefas".into(),
            account: Some("agente".into()),
            capabilities: vec!["tracker".into(), "tests".into()],
            settings: json!({ "project": "K" }),
        };
        for record in [code, issues] {
            store.save_connection(&record).expect("connection");
        }
    }

    pub async fn start(&self) -> Server {
        let data = self.data();
        let services = app::LocalServices::with_engine(Arc::new(self.agents.clone())).kept_in(&data);
        let client = app::Client::builder().services(services.clone()).build().expect("client");
        let keyed = Arc::new(app::KeyedEngine::new(services));
        let store = Arc::new(Store::open(&data.join("app.db"), 0).expect("store"));
        Hub::connections(&store);
        let tokens = Arc::new(MemoryTokenStore::new());
        for id in [CODE, tracker::CONNECTION] {
            tokens.save(&ConnectionId::from(id), &Grant::bearer("tok")).await.expect("grant");
        }
        let http: Arc<dyn connectors::Http> = Arc::clone(&self.github) as Arc<dyn connectors::Http>;
        let keeper = TokenKeeper::new(tokens, Arc::clone(&http), Arc::new(FixedClock::at_second(1_000)));
        let fake: &'static dyn Provider = self.tracker;
        let github =
            connectors::scripted::ScriptedProvider::load_in(None, "connectors/github/index.ts").expect("github");
        let providers: Vec<Arc<dyn Provider>> = vec![Arc::new(github), Arc::new(connectors::providers::Shared(fake))];
        let connectors = Arc::new(Connectors::from_providers(providers, http, keeper));
        let listed = content::builtin_files("").into_iter().map(|(path, _)| path.to_owned()).collect();
        let builtin: orchestrator::Builtin = Arc::new(content::builtin_text);
        let resources = Resources::new(Some(self.repo().join(brand::DIR)), None, builtin).listing(listed);
        let mut config = Config::new(Arc::clone(&store), Arc::clone(&connectors), keyed.clone(), data.clone())
            .with_resources(resources)
            .with_limits(limits())
            .with_sync_config(sync_config());
        config.repo = Some(self.repo());
        let handle = Orchestrator::start(config).expect("start");
        let backend = server::Backend {
            orchestrator: (*handle).clone(),
            store: Arc::clone(&store),
            connectors,
            client,
            engine: Some(keyed),
            data,
            browser: browser::Hub::new(),
        };
        let (router, _) = server::app(backend, &[], 7998);
        Server { handle, router, store }
    }
}

impl Server {
    pub async fn send(&self, method: Method, path: &str, body: Option<Value>) -> (StatusCode, Value) {
        let mut request = Request::builder().method(method).uri(path).header("host", "127.0.0.1:7998");
        let body = match body {
            Some(body) => {
                request = request.header("content-type", "application/json");
                Body::from(body.to_string())
            }
            None => Body::empty(),
        };
        let response = self.router.clone().oneshot(request.body(body).expect("request")).await.expect("response");
        let status = response.status();
        let bytes = response.into_body().collect().await.expect("body").to_bytes();
        let value =
            serde_json::from_slice(&bytes).unwrap_or(Value::String(String::from_utf8_lossy(&bytes).into_owned()));
        (status, value)
    }

    pub async fn get(&self, path: &str) -> Value {
        let (status, value) = self.send(Method::GET, path, None).await;
        assert_eq!(status, StatusCode::OK, "{path}: {value}");
        value
    }

    pub async fn post(&self, path: &str, body: Value) -> Value {
        let (status, value) = self.send(Method::POST, path, Some(body)).await;
        assert!(status.is_success(), "{path}: {status} {value}");
        value
    }

    pub async fn item(&self, id: &str) -> Value {
        let items = self.get("/api/items").await;
        items.as_array().and_then(|all| all.iter().find(|item| item["id"] == id)).cloned().unwrap_or(Value::Null)
    }

    pub async fn until(&self, id: &str, what: &str, check: impl Fn(&Value) -> bool) -> Value {
        for _ in 0..1500 {
            let item = self.item(id).await;
            if check(&item) {
                return item;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        panic!("{what} never happened: {}", self.item(id).await)
    }

    pub async fn asks(&self, id: &str) -> Vec<orchestrator::PendingAsk> {
        self.handle.asks().into_iter().filter(|ask| ask.item.as_str() == id).collect()
    }

    pub async fn ask(&self, id: &str, what: &str) -> orchestrator::PendingAsk {
        for _ in 0..1500 {
            if let Some(found) = self.asks(id).await.first() {
                return found.clone();
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        panic!("the agent never asked {what}: {}", self.item(id).await)
    }

    pub async fn respond(&self, ask: &orchestrator::PendingAsk, response: Value) {
        let session = ask.session.clone().expect("the ask has a session");
        let body = json!({ "event": ask.ask.id.0, "response": response });
        self.post(&format!("/api/sessions/{session}/respond"), body).await;
    }

    pub async fn input(&self, id: &str, step: &str, action: &str, payload: Value) {
        self.post(&format!("/api/items/{id}/steps/{step}/input"), json!({ "action": action, "payload": payload }))
            .await;
    }

    pub async fn create(&self, body: Value) -> String {
        let item = self.post("/api/items", body).await;
        item["id"].as_str().expect("id").to_owned()
    }

    pub async fn approve_plan(&self, id: &str) {
        let ask = self.ask(id, "the plan").await;
        assert!(matches!(ask.ask.kind, domain::AskKind::Plan { .. }), "{ask:?}");
        self.until(id, "the plan asks", |item| item["group"] == "you").await;
        self.respond(&ask, json!({ "kind": "plan", "decision": "approve" })).await;
        self.approve_cases(id).await;
    }

    pub async fn approve_cases(&self, id: &str) {
        let item = self.item(id).await;
        let has = item["steps"].as_array().is_some_and(|steps| steps.iter().any(|step| step["id"] == "cases"));
        if !has {
            return;
        }
        self.until(id, "the test cases wait", |item| waiting_on(item, "cases")).await;
        self.input(id, "cases", "approve_cases", json!({})).await;
    }

    pub async fn stop(self) {
        drop(self.handle);
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}

pub fn step<'a>(item: &'a Value, id: &str) -> &'a Value {
    item["steps"].as_array().and_then(|steps| steps.iter().find(|step| step["id"] == id)).unwrap_or(&Value::Null)
}

pub fn waiting_on(item: &Value, id: &str) -> bool {
    item["group"] == "you" && step(item, id)["state"] == "waiting_human"
}
