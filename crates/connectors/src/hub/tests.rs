use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, PoisonError};

use async_trait::async_trait;
use domain::connection::{Capability, Connection, ConnectionId, ProviderKey};
use serde_json::{Value, json};

use super::*;
use crate::clock::FixedClock;
use crate::descriptor::{ActionSpec, AuthSpec, object};
use crate::http::{FakeHttp, HttpRequest, Method, fetch};
use crate::oauth::{DeviceApp, Dialect, Grant, MemoryTokenStore, TokenStore};
use crate::provider::{EventSource, ExternalEvent, SettingsSink};

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
                ActionSpec::read("x.read", object(&[("a", "string")], &[]), json!({}), looked()),
                ActionSpec::read("y.read", object(&[], &[]), json!({}), looked()),
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
        _watch: &[String],
    ) -> Result<EventBatch, ActionError> {
        let event = |capability: &str| ExternalEvent {
            id: capability.to_owned(),
            connection: creds.connection.clone(),
            capability: Capability::from(capability),
            kind: format!("{capability}.changed"),
            subject: "s".into(),
            data: json!({}),
            at: jiff::Timestamp::UNIX_EPOCH,
            scope: String::new(),
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
async fn every_call_that_reaches_the_connector_is_counted_for_its_connection_and_origin() {
    let kept = Arc::new(crate::calls::kept::Kept::default());
    let http = FakeHttp::new().always(Method::Get, "/echo", 200, "{}");
    let hub =
        hub(http, Some(Grant::bearer("tok"))).await.with_calls(Arc::clone(&kept) as Arc<dyn crate::calls::CallSink>);
    let step = domain::origin::Attribution::of_step("i1", "develop", domain::origin::Origin::Step);
    let only_x = connection(&["x"]);
    assert!(hub.call_for(Some(&step), &only_x, &json!({}), "x.read", &json!({"a": "1"}), "k").await.is_ok());
    assert!(hub.call(&only_x, &json!({}), "x.read", &json!({"a": "1"}), "k").await.is_ok());
    assert!(hub.call(&only_x, &json!({}), "x.read", &json!({}), "k").await.is_err(), "refused before it is made");
    assert!(hub.poll(&connection(&["y"]), &json!({}), None, &[]).await.is_ok());
    let rows = kept.0.lock().expect("rows").clone();
    let seen: Vec<(&str, &str, Option<domain::origin::Origin>, bool)> = rows
        .iter()
        .map(|(connection, action, charged, ok)| {
            (connection.as_str(), action.as_str(), charged.as_ref().map(|c| c.origin), *ok)
        })
        .collect();
    let trigger = Some(domain::origin::Origin::Trigger);
    let step_origin = Some(domain::origin::Origin::Step);
    assert_eq!(
        seen,
        [("c1", "x.read", step_origin, true), ("c1", "x.read", None, true), ("c1", super::POLL, trigger, true)]
    );
    assert_eq!(rows[0].2.as_ref().and_then(|charged| charged.item.as_deref()), Some("i1"));
}

#[tokio::test]
async fn polling_keeps_only_events_of_capabilities_the_connection_offers() {
    let hub = hub(FakeHttp::new(), Some(Grant::bearer("tok"))).await;
    let batch = hub.poll(&connection(&["y"]), &json!({}), Some("c"), &[]).await;
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
    assert_eq!(hub.descriptors().len(), providers::all().len());
    for descriptor in hub.descriptors() {
        assert_eq!(hub.descriptor(&descriptor.key).map(|d| d.name.clone()), Ok(descriptor.name.clone()));
        assert!(descriptor.listing()["actions"].as_array().is_some_and(|a| !a.is_empty()));
    }
}
#[derive(Debug, Default)]
struct Picky {
    sites: Mutex<Vec<AccountOption>>,
}

#[async_trait]
impl Provider for Picky {
    fn descriptor(&self) -> ProviderDescriptor {
        let mut descriptor = Echo::default().descriptor();
        descriptor.key = ProviderKey::from("picky");
        descriptor
    }

    async fn call(&self, _: &dyn Http, creds: &Credentials, _: &str, _: &Value, _: &str) -> Result<Value, ActionError> {
        match creds.setting("site") {
            Some(site) => Ok(json!({ "site": site, "project": creds.settings["project"] })),
            None => Err(ActionError::blocked("Escolha o site.")),
        }
    }

    async fn accounts(&self, _: &dyn Http, _: &Credentials) -> Result<Vec<AccountOption>, ActionError> {
        Ok(self.sites.lock().unwrap_or_else(PoisonError::into_inner).clone())
    }

    fn chooses(&self) -> Vec<String> {
        vec!["site".to_owned()]
    }

    fn requires(&self) -> domain::data_policy::Requires {
        domain::data_policy::Requires { regions: Some(vec!["eu".into()]), ..Default::default() }
    }
}

#[derive(Debug, Default)]
struct Kept(Mutex<Vec<(ConnectionId, Value)>>);

impl SettingsSink for Kept {
    fn learned(&self, connection: &ConnectionId, settings: &Value) {
        self.0.lock().unwrap_or_else(PoisonError::into_inner).push((connection.clone(), settings.clone()));
    }
}

fn site(id: &str) -> AccountOption {
    AccountOption { id: id.into(), label: id.into(), settings: json!({ "site": id, "project": "P" }) }
}

async fn picky(sites: Vec<AccountOption>) -> (Connectors, Arc<Kept>, Connection) {
    let store = Arc::new(MemoryTokenStore::new());
    let _ = store.save(&ConnectionId::from("c1"), &Grant::bearer("tok")).await;
    let keeper = TokenKeeper::new(store, Arc::new(FakeHttp::new()), Arc::new(FixedClock::at_second(1_000)));
    let provider: &'static Picky = Box::leak(Box::new(Picky { sites: Mutex::new(sites) }));
    let provider: &'static dyn Provider = provider;
    let kept = Arc::new(Kept::default());
    let hub = Connectors::with_providers(&[provider], Arc::new(FakeHttp::new()), keeper)
        .with_sink(Arc::clone(&kept) as Arc<dyn SettingsSink>);
    let mut connection = connection(&["x"]);
    connection.provider = ProviderKey::from("picky");
    (hub, kept, connection)
}

#[tokio::test]
async fn a_connection_missing_its_choice_learns_the_only_option_and_keeps_it() {
    let (hub, kept, connection) = picky(vec![site("s1")]).await;
    assert_eq!(hub.chooses(&connection.provider), ["site"]);
    let settings = json!({ "project": "MINE" });
    let answer = hub.call(&connection, &settings, "x.read", &json!({"a": "1"}), "").await;
    assert_eq!(answer, Ok(json!({ "site": "s1", "project": "MINE" })));
    let kept = kept.0.lock().unwrap_or_else(PoisonError::into_inner).clone();
    assert_eq!(kept, vec![(ConnectionId::from("c1"), json!({ "site": "s1", "project": "MINE" }))]);
    let again = hub.call(&connection, &json!({}), "x.read", &json!({"a": "1"}), "").await;
    assert_eq!(again, Ok(json!({ "site": "s1", "project": "MINE" })));
}

#[tokio::test]
async fn a_connection_with_several_or_no_options_is_left_to_the_provider() {
    for sites in [vec![], vec![site("s1"), site("s2")]] {
        let (hub, kept, connection) = picky(sites).await;
        let refused = hub.call(&connection, &json!({}), "x.read", &json!({"a": "1"}), "").await;
        assert_eq!(refused.as_ref().map_err(ActionError::blocker), Err(Some("Escolha o site.")));
        assert!(kept.0.lock().unwrap_or_else(PoisonError::into_inner).is_empty());
    }
}

#[derive(Debug)]
struct Locked;

#[async_trait]
impl Provider for Locked {
    fn descriptor(&self) -> ProviderDescriptor {
        let mut descriptor = Echo::default().descriptor();
        descriptor.key = ProviderKey::from("locked");
        descriptor
    }

    async fn call(&self, _: &dyn Http, _: &Credentials, _: &str, _: &Value, _: &str) -> Result<Value, ActionError> {
        Err(ActionError::login_required("faça login no Helix"))
    }

    async fn accounts(&self, _: &dyn Http, _: &Credentials) -> Result<Vec<AccountOption>, ActionError> {
        Ok(Vec::new())
    }
}

#[tokio::test]
async fn a_login_required_error_is_shown_at_once_without_refreshing_the_token() {
    let store = Arc::new(MemoryTokenStore::new());
    let _ = store.save(&ConnectionId::from("c1"), &Grant::bearer("tok")).await;
    let keeper = TokenKeeper::new(store, Arc::new(FakeHttp::new()), Arc::new(FixedClock::at_second(1_000)));
    let provider: &'static dyn Provider = Box::leak(Box::new(Locked));
    let hub = Connectors::with_providers(&[provider], Arc::new(FakeHttp::new()), keeper);
    let mut connection = connection(&["x"]);
    connection.provider = ProviderKey::from("locked");
    let refused = hub.call(&connection, &json!({}), "x.read", &json!({"a": "1"}), "").await;
    assert!(matches!(refused, Err(e) if e.needs_login() && e.shown() == "faça login no Helix"));
}

fn looked() -> domain::operation::Profile {
    use domain::operation::{Kind, Profile, Reversible, Risk};
    Profile::new(vec![Kind::Read], Risk::Low, Reversible::Yes, false, false)
}

#[tokio::test]
async fn the_hub_hands_out_what_each_connector_requires_and_nothing_for_others() {
    let (hub, _, _) = picky(Vec::new()).await;
    let eu = domain::data_policy::Requires { regions: Some(vec!["eu".into()]), ..Default::default() };
    assert_eq!(hub.requires(&ProviderKey::from("picky")), eu);
    assert!(hub.requires(&ProviderKey::from("nobody")).is_empty());
    assert!(self::hub(FakeHttp::new(), None).await.requires(&ProviderKey::from("echo")).is_empty());
}
