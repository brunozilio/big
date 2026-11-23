use std::sync::atomic::{AtomicUsize, Ordering};

use async_trait::async_trait;
use domain::connection::{Capability, Connection, ConnectionId, ProviderKey};
use serde_json::{Value, json};

use super::*;
use crate::clock::FixedClock;
use crate::descriptor::{ActionSpec, AuthSpec, object};
use crate::http::{FakeHttp, HttpRequest, Method, fetch};
use crate::oauth::{DeviceApp, Dialect, Grant, MemoryTokenStore, TokenStore};
use crate::provider::{EventSource, ExternalEvent};

#[derive(Debug, Default)]
struct Echo {
    calls: AtomicUsize,
}

fn device() -> DeviceApp {
    DeviceApp {
        client_id: "c".into(),
        client_secret: None,
        device_endpoint: "https://login.test/device".into(),
        token_endpoint: "https://login.test/token".into(),
        scopes: Vec::new(),
        start_fields: Vec::new(),
        dialect: Dialect::Form,
    }
}

#[async_trait]
impl Provider for Echo {
    fn descriptor(&self) -> ProviderDescriptor {
        ProviderDescriptor {
            key: ProviderKey::from("echo"),
            name: "Echo".into(),
            logo: "<svg/>".into(),
            capabilities: vec![Capability::from("x"), Capability::from("y")],
            auth: AuthSpec::Device(device()),
            actions: vec![
                ActionSpec::read("x.read", object(&[("a", "string")], &[]), json!({})),
                ActionSpec::read("y.read", object(&[], &[]), json!({})),
            ],
        }
    }

    async fn call(
        &self,
        http: &dyn Http,
        creds: &Credentials,
        action: &str,
        args: &Value,
        key: &str,
    ) -> Result<Value, ActionError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        fetch(http, HttpRequest::get("https://api.test/echo").bearer(&creds.access_token)).await?;
        Ok(
            json!({ "action": action, "args": args, "key": key, "token": creds.access_token, "settings": creds.settings }),
        )
    }

    async fn accounts(&self, _http: &dyn Http, creds: &Credentials) -> Result<Vec<AccountOption>, ActionError> {
        Ok(vec![AccountOption { id: creds.access_token.clone(), label: "me".into(), settings: json!({}) }])
    }

    fn events(&self) -> Option<&dyn EventSource> {
        Some(self)
    }
}

#[async_trait]
impl EventSource for Echo {
    async fn poll(
        &self,
        _http: &dyn Http,
        creds: &Credentials,
        cursor: Option<&str>,
    ) -> Result<EventBatch, ActionError> {
        let event = |capability: &str| ExternalEvent {
            id: capability.to_owned(),
            connection: creds.connection.clone(),
            capability: Capability::from(capability),
            kind: format!("{capability}.changed"),
            subject: "s".into(),
            data: json!({}),
            at: jiff::Timestamp::UNIX_EPOCH,
        };
        Ok(EventBatch { events: vec![event("x"), event("y")], cursor: format!("{}+", cursor.unwrap_or_default()) })
    }
}

static ECHO: std::sync::LazyLock<Echo> = std::sync::LazyLock::new(Echo::default);

fn connection(capabilities: &[&str]) -> Connection {
    Connection {
        id: ConnectionId::from("c1"),
        provider: ProviderKey::from("echo"),
        label: "Echo".into(),
        account: "me".into(),
        capabilities: capabilities.iter().map(|name| Capability::from(*name)).collect(),
    }
}

async fn hub(http: FakeHttp, grant: Option<Grant>) -> Connectors {
    let store = Arc::new(MemoryTokenStore::new());
    if let Some(grant) = grant {
        let _ = store.save(&ConnectionId::from("c1"), &grant).await;
    }
    let keeper = TokenKeeper::new(store, Arc::new(FakeHttp::new()), Arc::new(FixedClock::at_second(1_000)));
    let echo: &'static dyn Provider = &*ECHO;
    Connectors::with_providers(&[echo], Arc::new(http), keeper)
}

#[tokio::test]
async fn a_call_reaches_the_provider_with_the_kept_token_and_the_settings() {
    let hub = hub(FakeHttp::new().always(Method::Get, "/echo", 200, "{}"), Some(Grant::bearer("tok"))).await;
    let answer = hub.call(&connection(&["x"]), &json!({"site": "s"}), "x.read", &json!({"a": "1"}), "k1").await;
    let Ok(answer) = answer else { panic!("{answer:?}") };
    assert_eq!(answer["token"], "tok");
    assert_eq!(answer["key"], "k1");
    assert_eq!(answer["settings"]["site"], "s");
}

#[tokio::test]
async fn unknown_actions_missing_arguments_and_capabilities_outside_the_connection_are_permanent() {
    let hub = hub(FakeHttp::new(), Some(Grant::bearer("tok"))).await;
    let only_x = connection(&["x"]);
    for (action, args) in [("x.nope", json!({"a": "1"})), ("x.read", json!({})), ("y.read", json!({}))] {
        let refused = hub.call(&only_x, &json!({}), action, &args, "").await;
        assert_eq!(refused.map_err(|e| e.class), Err(ErrorClass::Permanent), "{action}");
    }
    let mut other = connection(&["x"]);
    other.provider = ProviderKey::from("missing");
    assert_eq!(
        hub.call(&other, &json!({}), "x.read", &json!({"a": "1"}), "").await.map_err(|e| e.class),
        Err(ErrorClass::Permanent)
    );
}

#[tokio::test]
async fn a_connection_without_a_token_asks_to_reconnect() {
    let hub = hub(FakeHttp::new(), None).await;
    let refused = hub.call(&connection(&["x"]), &json!({}), "x.read", &json!({"a": "1"}), "").await;
    assert!(matches!(refused, Err(e) if e.class == ErrorClass::Auth && e.message.starts_with(RECONNECT)));
}

#[tokio::test]
async fn a_rejected_token_without_refresh_asks_to_reconnect_after_one_attempt() {
    let hub = hub(FakeHttp::new().always(Method::Get, "/echo", 401, "{}"), Some(Grant::bearer("tok"))).await;
    let before = ECHO.calls.load(Ordering::SeqCst);
    let refused = hub.call(&connection(&["x"]), &json!({}), "x.read", &json!({"a": "1"}), "").await;
    assert!(matches!(refused, Err(e) if e.class == ErrorClass::Auth && e.message.starts_with(RECONNECT)));
    assert!(ECHO.calls.load(Ordering::SeqCst) > before);
}

#[tokio::test(start_paused = true)]
async fn a_rate_limited_call_holds_the_connection() {
    let limited = crate::http::HttpResponse::new(429, "{}").with_header("retry-after", "30");
    let hub = hub(FakeHttp::new().respond(Method::Get, "/echo", limited), Some(Grant::bearer("tok"))).await;
    let refused = hub.call(&connection(&["x"]), &json!({}), "x.read", &json!({"a": "1"}), "").await;
    assert!(refused.is_err_and(|e| e.retryable()));
    assert!(hub.limiter().waiting(&ConnectionId::from("c1")).is_some());
}

#[tokio::test]
async fn polling_keeps_only_events_of_capabilities_the_connection_offers() {
    let hub = hub(FakeHttp::new(), Some(Grant::bearer("tok"))).await;
    let batch = hub.poll(&connection(&["y"]), &json!({}), Some("c")).await;
    let Ok(Some(batch)) = batch else { panic!("{batch:?}") };
    assert_eq!(batch.events.iter().map(|e| e.kind.as_str()).collect::<Vec<_>>(), ["y.changed"]);
    assert_eq!(batch.cursor, "c+");
    let accounts = hub.accounts(&connection(&["y"]), &json!({})).await;
    assert_eq!(accounts.map(|a| a[0].id.clone()), Ok("tok".to_owned()));
}

#[test]
fn the_default_registry_lists_every_provider() {
    let keeper = TokenKeeper::new(
        Arc::new(MemoryTokenStore::new()),
        Arc::new(FakeHttp::new()),
        Arc::new(FixedClock::at_second(0)),
    );
    let hub = Connectors::new(Arc::new(FakeHttp::new()), keeper);
    assert_eq!(hub.descriptors().len(), providers::ALL.len());
    for descriptor in hub.descriptors() {
        assert_eq!(hub.descriptor(&descriptor.key).map(|d| d.name.clone()), Ok(descriptor.name.clone()));
        assert!(descriptor.listing()["actions"].as_array().is_some_and(|a| !a.is_empty()));
    }
}
