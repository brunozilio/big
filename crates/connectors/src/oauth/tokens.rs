use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError};

use async_trait::async_trait;
use domain::connection::ConnectionId;

use super::Grant;
use crate::clock::Clock;
use crate::descriptor::AlternateAuth;
use crate::descriptor::AuthSpec;
use crate::error::{ActionError, ErrorClass, RECONNECT};
use crate::http::Http;
use crate::provider::auth_for;

pub const REFRESH_WINDOW_SECONDS: i64 = 300;

#[async_trait]
pub trait TokenStore: Send + Sync + std::fmt::Debug {
    async fn load(&self, connection: &ConnectionId) -> Result<Option<Grant>, ActionError>;
    async fn save(&self, connection: &ConnectionId, grant: &Grant) -> Result<(), ActionError>;
    async fn remove(&self, connection: &ConnectionId) -> Result<(), ActionError>;
}

#[derive(Debug, Default)]
pub struct MemoryTokenStore(Mutex<HashMap<ConnectionId, Grant>>);

impl MemoryTokenStore {
    pub fn new() -> MemoryTokenStore {
        MemoryTokenStore::default()
    }

    fn held(&self) -> std::sync::MutexGuard<'_, HashMap<ConnectionId, Grant>> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

#[async_trait]
impl TokenStore for MemoryTokenStore {
    async fn load(&self, connection: &ConnectionId) -> Result<Option<Grant>, ActionError> {
        Ok(self.held().get(connection).cloned())
    }

    async fn save(&self, connection: &ConnectionId, grant: &Grant) -> Result<(), ActionError> {
        self.held().insert(connection.clone(), grant.clone());
        Ok(())
    }

    async fn remove(&self, connection: &ConnectionId) -> Result<(), ActionError> {
        self.held().remove(connection);
        Ok(())
    }
}

type Locks = Mutex<HashMap<ConnectionId, Arc<tokio::sync::Mutex<()>>>>;

#[derive(Debug)]
pub struct TokenKeeper {
    store: Arc<dyn TokenStore>,
    http: Arc<dyn Http>,
    clock: Arc<dyn Clock>,
    locks: Locks,
}

fn reconnect(reason: &str) -> ActionError {
    ActionError::auth(format!("{RECONNECT}: {reason}"))
}

impl TokenKeeper {
    pub fn new(store: Arc<dyn TokenStore>, http: Arc<dyn Http>, clock: Arc<dyn Clock>) -> TokenKeeper {
        TokenKeeper { store, http, clock, locks: Mutex::default() }
    }

    pub fn store(&self) -> &dyn TokenStore {
        self.store.as_ref()
    }

    pub async fn keep(&self, connection: &ConnectionId, grant: &Grant) -> Result<(), ActionError> {
        self.store.save(connection, grant).await
    }

    pub async fn access_token(&self, connection: &ConnectionId, auth: &AuthSpec) -> Result<String, ActionError> {
        self.access_token_of(connection, auth, &[]).await
    }

    pub async fn access_token_of(
        &self,
        connection: &ConnectionId,
        auth: &AuthSpec,
        alternatives: &[AlternateAuth],
    ) -> Result<String, ActionError> {
        let grant = self.store.load(connection).await?.ok_or_else(|| reconnect("the connection has no token"))?;
        if !grant.stale(self.clock.now()) {
            return Ok(grant.access_token);
        }
        self.refreshed_of(connection, auth, alternatives, &grant.access_token).await
    }

    pub async fn refreshed(
        &self,
        connection: &ConnectionId,
        auth: &AuthSpec,
        stale: &str,
    ) -> Result<String, ActionError> {
        self.refreshed_of(connection, auth, &[], stale).await
    }

    pub async fn refreshed_of(
        &self,
        connection: &ConnectionId,
        primary: &AuthSpec,
        alternatives: &[AlternateAuth],
        stale: &str,
    ) -> Result<String, ActionError> {
        let lock = {
            let mut all = self.locks.lock().unwrap_or_else(PoisonError::into_inner);
            Arc::clone(all.entry(connection.clone()).or_default())
        };
        let _held = lock.lock().await;
        let now = self.clock.now();
        let grant = self.store.load(connection).await?.ok_or_else(|| reconnect("the connection has no token"))?;
        if grant.access_token != stale && !grant.stale(now) {
            return Ok(grant.access_token);
        }
        let refresh_token =
            grant.refresh_token.clone().ok_or_else(|| reconnect("the token expired and cannot be renewed"))?;
        let auth = auth_for(primary, alternatives, grant.method.as_deref());
        let mut fresh = match self.exchange(&auth, &grant, &refresh_token, now).await {
            Ok(fresh) => fresh,
            Err(error) if matches!(error.class, ErrorClass::Auth | ErrorClass::Permanent) => {
                return Err(reconnect(&error.message));
            }
            Err(error) => return Err(error),
        };
        fresh.client = fresh.client.or(grant.client);
        fresh.method = fresh.method.or(grant.method);
        if fresh.extra.is_null() {
            fresh.extra = grant.extra;
        }
        self.store.save(connection, &fresh).await?;
        Ok(fresh.access_token)
    }

    async fn exchange(
        &self,
        auth: &AuthSpec,
        grant: &Grant,
        refresh_token: &str,
        now: jiff::Timestamp,
    ) -> Result<Grant, ActionError> {
        let http = self.http.as_ref();
        match auth {
            AuthSpec::Device(app) => app.refresh(http, refresh_token, now).await,
            AuthSpec::Sso(spec) => {
                let client = grant.client.as_ref().ok_or_else(|| reconnect("the registered client is missing"))?;
                let region = grant.extra["region"].as_str().unwrap_or_default();
                spec.in_region(region).device_for(client, "").refresh(http, refresh_token, now).await
            }
            AuthSpec::Loopback(sign_in) => sign_in.0.refresh(http, grant, now).await,
            AuthSpec::Token(_) => Err(reconnect("the API token was refused")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clock::{FixedClock, after_seconds};
    use crate::http::{FakeHttp, Method};
    use crate::oauth::{DeviceApp, Dialect};

    fn device() -> AuthSpec {
        AuthSpec::Device(DeviceApp {
            client_id: "c".into(),
            client_secret: Some("s".into()),
            device_endpoint: "https://a/code".into(),
            token_endpoint: "https://a/token".into(),
            scopes: Vec::new(),
            start_fields: Vec::new(),
            dialect: Dialect::Form,
        })
    }

    fn keeper(http: FakeHttp, clock: Arc<FixedClock>) -> (TokenKeeper, Arc<MemoryTokenStore>, Arc<FakeHttp>) {
        let store = Arc::new(MemoryTokenStore::new());
        let http = Arc::new(http);
        (TokenKeeper::new(store.clone(), http.clone(), clock), store, http)
    }

    fn expiring(token: &str, clock: &FixedClock, seconds: i64) -> Grant {
        Grant {
            expires_at: Some(after_seconds(clock.now(), seconds)),
            refresh_token: Some("rt".into()),
            ..Grant::bearer(token)
        }
    }

    #[tokio::test]
    async fn a_token_far_from_expiry_is_used_as_is() {
        let clock = Arc::new(FixedClock::at_second(1_000));
        let (keeper, store, http) = keeper(FakeHttp::new(), clock.clone());
        let id = ConnectionId::from("c1");
        let _ = store.save(&id, &expiring("old", &clock, 3600)).await;
        assert_eq!(keeper.access_token(&id, &device()).await, Ok("old".to_owned()));
        assert!(http.sent().is_empty());
    }

    #[tokio::test]
    async fn a_token_inside_the_five_minute_window_is_refreshed_and_stored() {
        let clock = Arc::new(FixedClock::at_second(1_000));
        let answer = r#"{"access_token":"new","refresh_token":"rt2","expires_in":3600}"#;
        let (keeper, store, http) = keeper(FakeHttp::new().once(Method::Post, "/token", 200, answer), clock.clone());
        let id = ConnectionId::from("c1");
        let _ = store.save(&id, &expiring("old", &clock, 299)).await;
        assert_eq!(keeper.access_token(&id, &device()).await, Ok("new".to_owned()));
        let kept = store.load(&id).await.ok().flatten();
        assert_eq!(kept.and_then(|g| g.refresh_token), Some("rt2".to_owned()));
        assert_eq!(keeper.access_token(&id, &device()).await, Ok("new".to_owned()));
        assert_eq!(http.sent().len(), 1);
    }

    #[tokio::test]
    async fn a_refused_refresh_asks_to_reconnect() {
        let clock = Arc::new(FixedClock::at_second(1_000));
        let http = FakeHttp::new().once(Method::Post, "/token", 400, r#"{"error":"invalid_grant"}"#);
        let (keeper, store, _) = keeper(http, clock.clone());
        let id = ConnectionId::from("c1");
        let _ = store.save(&id, &expiring("old", &clock, 10)).await;
        let refused = keeper.access_token(&id, &device()).await;
        assert!(matches!(&refused, Err(e) if e.class == ErrorClass::Auth && e.message.starts_with(RECONNECT)));
    }

    #[tokio::test]
    async fn a_refresh_that_could_not_reach_the_service_stays_transient() {
        let clock = Arc::new(FixedClock::at_second(1_000));
        let http = FakeHttp::new().once(Method::Post, "/token", 502, "bad gateway");
        let (keeper, store, _) = keeper(http, clock.clone());
        let id = ConnectionId::from("c1");
        let _ = store.save(&id, &expiring("old", &clock, 10)).await;
        assert_eq!(keeper.access_token(&id, &device()).await.map_err(|e| e.class), Err(ErrorClass::Transient));
    }

    #[tokio::test]
    async fn a_missing_token_or_refresh_token_asks_to_reconnect() {
        let clock = Arc::new(FixedClock::at_second(1_000));
        let (keeper, store, _) = keeper(FakeHttp::new(), clock.clone());
        let id = ConnectionId::from("c1");
        assert_eq!(keeper.access_token(&id, &device()).await.map_err(|e| e.class), Err(ErrorClass::Auth));
        let _ = store.save(&id, &Grant { expires_at: Some(clock.now()), ..Grant::bearer("t") }).await;
        assert_eq!(keeper.access_token(&id, &device()).await.map_err(|e| e.class), Err(ErrorClass::Auth));
    }

    #[tokio::test]
    async fn a_forced_refresh_after_another_caller_renewed_reuses_the_new_token() {
        let clock = Arc::new(FixedClock::at_second(1_000));
        let (keeper, store, http) = keeper(FakeHttp::new(), clock.clone());
        let id = ConnectionId::from("c1");
        let _ = store.save(&id, &expiring("renewed", &clock, 3600)).await;
        let device = AuthSpec::Device(DeviceApp {
            client_id: "c".into(),
            client_secret: None,
            device_endpoint: "https://d/code".into(),
            token_endpoint: "https://d/token".into(),
            scopes: Vec::new(),
            start_fields: Vec::new(),
            dialect: Dialect::Form,
        });
        assert_eq!(keeper.refreshed(&id, &device, "stale").await, Ok("renewed".to_owned()));
        assert!(http.sent().is_empty());
    }
}
