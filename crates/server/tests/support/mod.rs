#![allow(clippy::expect_used, dead_code, reason = "a failed setup step stops the test")]

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use agent_engine::fake::FakeEngine;
use async_trait::async_trait;
use axum::Router;
use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use connectors::descriptor::object;
use connectors::oauth::Dialect;
use connectors::{
    AccountOption, ActionError, ActionSpec, AuthSpec, Connectors, Credentials, DeviceApp, FakeHttp, FixedClock, Grant,
    Http, Provider, ProviderDescriptor, TokenKeeper, TokenStore,
};
use domain::connection::{Capability, ConnectionId, ProviderKey};
use domain::{EngineEvent, ModelOption, SessionId};
use http_body_util::BodyExt;
use orchestrator::{Config, Handle, Orchestrator, Resources};
use serde_json::{Value, json};
use store::{ConnectionRecord, Store};
use tower::ServiceExt;

pub const SESSION: &str = "s-work";

pub const WORKFLOW: &str = r#"
import { workflow, step, chat, markdown, actions } from "sdk";

const work = step({
  name: "work",
  label: "Trabalho",
  icon: "sCode",
  doing: "Trabalhando",
  view: (s) => (s.session ? [chat(s.session, { composer: true })] : []),
  async run(ctx) {
    const done = await ctx.agent("worker", "faça");
    return ctx.pass({ session: done.session });
  },
});

const approve = step({
  name: "approve",
  label: "Aprovação",
  icon: "check",
  needs: "tracker",
  view: () => [markdown("aprovado")],
  async run(ctx) {
    const answer = await ctx.waitHuman([markdown("Aprovar?"), actions(["approve"])]);
    const issue = await ctx.call("tracker", "issue", { issue: "K-1" });
    return ctx.pass({ by: answer.action, issue: issue.ref, url: issue.url, pr: "o/r#3" });
  },
});

export default workflow({ name: "default", steps: [work(), approve()] });
"#;

const WORKER: &str = "---\nname: worker\npermissionMode: bypass\nmodel: auto\n---\nVocê trabalha.";

#[derive(Debug, Default)]
pub struct FakeProvider {
    pub calls: Mutex<Vec<(String, Value)>>,
}

const ACTIONS: [&str; 8] = [
    "code.repos",
    "tracker.issue",
    "tracker.search",
    "tracker.projects",
    "tracker.comment",
    "code.pull_request",
    "code.files_diff",
    "code.review_comments",
];

#[async_trait]
impl Provider for FakeProvider {
    fn descriptor(&self) -> ProviderDescriptor {
        ProviderDescriptor {
            key: ProviderKey::from("fake"),
            name: "Fake".into(),
            logo: "<svg/>".into(),
            capabilities: vec![Capability::from("tracker"), Capability::from("code"), Capability::from("pipelines")],
            auth: AuthSpec::Device(DeviceApp {
                client_id: "c".into(),
                client_secret: None,
                device_endpoint: "https://login.test/device".into(),
                token_endpoint: "https://login.test/token".into(),
                scopes: Vec::new(),
                start_fields: Vec::new(),
                dialect: Dialect::Form,
            }),
            actions: ACTIONS
                .iter()
                .chain(["pipelines.run"].iter())
                .map(|name| ActionSpec::read(name, object(&[], &[]), json!({})))
                .collect(),
        }
    }

    async fn call(
        &self,
        _: &dyn Http,
        _: &Credentials,
        action: &str,
        args: &Value,
        _: &str,
    ) -> Result<Value, ActionError> {
        self.calls.lock().expect("lock").push((action.to_owned(), args.clone()));
        Ok(match action {
            "tracker.issue" => {
                json!({ "ref": "K-1", "url": "https://tracker.test/K-1", "title": "Corrigir", "status": "A fazer",
                "description": "texto", "subtasks": [], "attachments": [], "comments": [] })
            }
            "tracker.search" => json!({ "issues": [{ "ref": "K-1", "title": "Corrigir", "status": "A fazer" }] }),
            "tracker.projects" => json!([
                { "id": "1", "key": "APP", "name": "Aplicativo", "avatar": "https://tracker.test/app.png" },
                { "id": "2", "key": "OPS", "name": "Operações" },
            ]),
            "tracker.comment" => json!({ "id": "9", "created": true }),
            "code.pull_request" => {
                json!({ "ref": "o/r#3", "number": 3, "title": "PR", "url": "https://code.test/3", "state": "open", "head": "h", "base": "main", "reviews": [] })
            }
            "code.files_diff" => {
                json!([{ "path": "a.rs", "change": "added", "additions": 1, "deletions": 0, "patch": "@@ -0,0 +1 @@\n+x" }])
            }
            "code.review_comments" => json!([]),
            "code.repos" => json!([
                { "id": "10", "ref": "acme/web", "name": "web", "url": "https://code.test/acme/web",
                  "default_branch": "main", "private": true },
            ]),
            "pipelines.run" => json!({ "ref": "o/r/runs/9", "pipeline": "CI", "state": "passed", "jobs": [] }),
            _ => json!({}),
        })
    }

    async fn accounts(&self, _: &dyn Http, _: &Credentials) -> Result<Vec<AccountOption>, ActionError> {
        Ok(vec![
            AccountOption { id: "a".into(), label: "conta-a".into(), settings: json!({ "site": "a.test" }) },
            AccountOption { id: "b".into(), label: "conta-b".into(), settings: json!({ "site": "b.test" }) },
        ])
    }
}

#[derive(Debug, Default)]
pub struct Picky;

#[async_trait]
impl Provider for Picky {
    fn descriptor(&self) -> ProviderDescriptor {
        let mut descriptor = FakeProvider::default().descriptor();
        descriptor.key = ProviderKey::from("picky");
        descriptor.name = "Picky".into();
        descriptor.capabilities = vec![Capability::from("tracker")];
        descriptor.actions = vec![ActionSpec::read("tracker.search", object(&[], &[]), json!({}))];
        descriptor
    }

    async fn call(&self, _: &dyn Http, creds: &Credentials, _: &str, _: &Value, _: &str) -> Result<Value, ActionError> {
        let site = creds.setting("site").ok_or_else(|| ActionError::blocked("Reconecte e escolha o site."))?;
        Ok(json!({ "issues": [{ "ref": "S-1", "title": site, "status": "A fazer" }] }))
    }

    async fn accounts(&self, _: &dyn Http, creds: &Credentials) -> Result<Vec<AccountOption>, ActionError> {
        if creds.access_token == "novo" {
            return Ok(Vec::new());
        }
        Ok(vec![AccountOption { id: "s".into(), label: "only.test".into(), settings: json!({ "site": "only.test" }) }])
    }

    fn chooses(&self) -> &'static [&'static str] {
        &["site"]
    }
}

pub struct World {
    pub store: Arc<Store>,
    pub connectors: Arc<Connectors>,
    pub dir: tempfile::TempDir,
    pub provider: &'static FakeProvider,
    pub engine: FakeEngine,
    pub handle: Handle,
    pub router: Router,
    pub ctx_live: server::Live,
}

fn script() -> Vec<EngineEvent> {
    vec![
        EngineEvent::SessionStarted(SessionId(SESSION.into())),
        EngineEvent::TextDelta("feito".into()),
        EngineEvent::Completed { duration_ms: 0, cost_usd: None },
    ]
}

fn http() -> FakeHttp {
    FakeHttp::new()
        .always(connectors::Method::Post, "/device", 200, r#"{"device_code":"dc","user_code":"ABCD-EFGH","verification_uri":"https://login.test/activate","expires_in":900,"interval":1}"#)
        .once(connectors::Method::Post, "/token", 400, r#"{"error":"authorization_pending"}"#)
        .always(connectors::Method::Post, "/token", 200, r#"{"access_token":"novo","expires_in":3600}"#)
}

pub fn repo(dir: &Path) -> PathBuf {
    let repo = dir.join("repo");
    let files = [("workflows/default.ts", WORKFLOW), ("agents/worker.md", WORKER)];
    for (path, text) in files {
        let file = repo.join(brand::DIR).join(path);
        std::fs::create_dir_all(file.parent().expect("parent")).expect("dir");
        std::fs::write(file, text).expect("write");
    }
    repo
}

impl World {
    pub async fn new() -> World {
        let dir = tempfile::tempdir().expect("dir");
        let data = dir.path().join("data");
        std::fs::create_dir_all(&data).expect("data");
        let model = ModelOption::new("m1", "Modelo Um");
        let engine = FakeEngine::new(vec![model], script());
        let services = app::LocalServices::with_engine(Arc::new(engine.clone())).kept_in(&data);
        let client = app::Client::builder().services(services.clone()).build().expect("client");
        let keyed = Arc::new(app::KeyedEngine::new(services.clone()));
        let store = Arc::new(Store::open(&data.join("app.db"), 0).expect("store"));
        store
            .save_connection(&ConnectionRecord {
                id: "c1".into(),
                provider: "fake".into(),
                label: "Fake".into(),
                account: Some("eu".into()),
                capabilities: vec!["tracker".into(), "code".into(), "pipelines".into()],
                settings: json!({}),
            })
            .expect("connection");
        let tokens = Arc::new(server::VaultTokens::new(Arc::clone(&services.secrets)));
        tokens.save(&ConnectionId::from("c1"), &Grant::bearer("tok")).await.expect("grant");
        let http = Arc::new(http());
        let keeper = TokenKeeper::new(tokens, http.clone(), Arc::new(FixedClock::at_second(1_000)));
        let provider: &'static FakeProvider = Box::leak(Box::new(FakeProvider::default()));
        let fake: &'static dyn Provider = provider;
        let picky: &'static dyn Provider = &Picky;
        let sink = Arc::new(server::StoredSettings(Arc::clone(&store)));
        let connectors = Arc::new(Connectors::with_providers(&[fake, picky], http, keeper).with_sink(sink));
        let listed = content::builtin_files("").into_iter().map(|(path, _)| path.to_owned()).collect();
        let builtin: orchestrator::Builtin = Arc::new(content::builtin_text);
        let resources = Resources::new(Some(repo(dir.path()).join(brand::DIR)), None, builtin).listing(listed);
        let mut config = Config::new(Arc::clone(&store), Arc::clone(&connectors), keyed.clone(), data.clone())
            .with_resources(resources)
            .without_sync();
        config.repo = Some(repo(dir.path()));
        let handle = Orchestrator::start(config).expect("start");
        let backend = server::Backend {
            orchestrator: (*handle).clone(),
            store: Arc::clone(&store),
            connectors: Arc::clone(&connectors),
            client,
            engine: Some(keyed),
            data,
        };
        let (router, ctx_live) = server::app(backend, BUNDLE, 7999);
        World { store, connectors, dir, provider, engine, handle, router, ctx_live }
    }

    pub async fn send(&self, method: Method, path: &str, body: Option<Value>) -> (StatusCode, Value) {
        let mut request = Request::builder().method(method).uri(path).header("host", "127.0.0.1:7999");
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
        let value = if bytes.is_empty() {
            Value::Null
        } else {
            serde_json::from_slice(&bytes).unwrap_or(Value::String(String::from_utf8_lossy(&bytes).into_owned()))
        };
        (status, value)
    }

    pub async fn get(&self, path: &str) -> Value {
        let (status, value) = self.send(Method::GET, path, None).await;
        assert_eq!(status, StatusCode::OK, "{path}: {value}");
        value
    }

    pub async fn until(&self, what: &str, check: impl Fn(&Value) -> bool) -> Value {
        for _ in 0..400 {
            let items = self.get("/api/items").await;
            if let Some(found) = items.as_array().and_then(|all| all.iter().find(|item| check(item))) {
                return found.clone();
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        panic!("never reached {what}: {}", self.get("/api/items").await)
    }
}

pub static BUNDLE: &[server::Asset] = &[
    server::Asset { path: "index.html", bytes: b"<!doctype html><title>page</title>" },
    server::Asset { path: "assets/app.js", bytes: b"console.log(1)" },
];

pub fn fixture(name: &str, value: &Value) {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../web/test/fixtures/server");
    std::fs::create_dir_all(&dir).expect("fixtures");
    let text = serde_json::to_string_pretty(&steady(value.clone())).expect("json");
    let file = dir.join(format!("{name}.json"));
    if std::fs::read_to_string(&file).ok().as_deref() != Some(text.as_str()) {
        std::fs::write(file, text).expect("fixture");
    }
}

fn steady(value: Value) -> Value {
    match value {
        Value::String(text) if text.parse::<jiff::Timestamp>().is_ok() => json!("2026-01-01T00:00:00Z"),
        Value::String(text) if text.len() == 36 && text.chars().filter(|c| *c == '-').count() == 4 => json!("item-1"),
        Value::String(text) if text.starts_with("data:image/") => json!("data:image/svg+xml;base64,PHN2Zy8+"),
        Value::Array(all) => Value::Array(all.into_iter().map(steady).collect()),
        Value::Object(fields) => Value::Object(
            fields
                .into_iter()
                .map(|(key, value)| {
                    let kept = if key == "seconds" { json!(0) } else { steady(value) };
                    (key, kept)
                })
                .collect(),
        ),
        other => other,
    }
}
