use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError};

use domain::Language;
use domain::connection::{Capability, Connection, ConnectionId, ProviderKey};
use serde_json::{Map, Value};

use crate::descriptor::{AlternateAuth, Presentation, ProviderDescriptor};
use crate::error::{ActionError, ErrorClass, RECONNECT};
use crate::http::Http;
use crate::limiter::{Metered, RateLimiter};
use crate::oauth::TokenKeeper;
use crate::provider::{AccountOption, Credentials, EventBatch, Provider, SettingsSink, unknown_action};
use crate::providers;

#[derive(Debug)]
struct Entry {
    provider: Arc<dyn Provider>,
    descriptor: ProviderDescriptor,
    alternatives: Vec<AlternateAuth>,
}

#[derive(Debug)]
pub struct Connectors {
    entries: Vec<Entry>,
    http: Arc<dyn Http>,
    keeper: TokenKeeper,
    limiter: RateLimiter,
    learned: Mutex<HashMap<ConnectionId, Value>>,
    sink: Option<Arc<dyn SettingsSink>>,
}

fn present(value: Option<&Value>) -> bool {
    match value {
        None | Some(Value::Null) => false,
        Some(Value::String(text)) => !text.is_empty(),
        Some(_) => true,
    }
}

fn has(settings: &Value, name: &str) -> bool {
    present(settings.get(name))
}

pub fn filled(settings: &Value, extra: &Value) -> Value {
    let mut merged: Map<String, Value> = settings.as_object().cloned().unwrap_or_default();
    for (key, value) in extra.as_object().into_iter().flatten() {
        if !present(merged.get(key)) && present(Some(value)) {
            merged.insert(key.clone(), value.clone());
        }
    }
    Value::Object(merged)
}

impl Connectors {
    pub fn new(http: Arc<dyn Http>, keeper: TokenKeeper) -> Connectors {
        Connectors::from_providers(providers::all(), http, keeper)
    }

    pub fn with_providers(providers: &[&'static dyn Provider], http: Arc<dyn Http>, keeper: TokenKeeper) -> Connectors {
        let shared = providers.iter().map(|provider| Arc::new(providers::Shared(*provider)) as Arc<dyn Provider>);
        Connectors::from_providers(shared.collect(), http, keeper)
    }

    pub fn from_providers(providers: Vec<Arc<dyn Provider>>, http: Arc<dyn Http>, keeper: TokenKeeper) -> Connectors {
        let entries = providers
            .into_iter()
            .map(|provider| Entry {
                descriptor: provider.descriptor(),
                alternatives: provider.alternatives(Language::default()),
                provider,
            })
            .collect();
        Connectors { entries, http, keeper, limiter: RateLimiter::new(), learned: Mutex::default(), sink: None }
    }

    pub fn with_sink(mut self, sink: Arc<dyn SettingsSink>) -> Connectors {
        self.sink = Some(sink);
        self
    }

    pub fn chooses(&self, key: &ProviderKey) -> Vec<String> {
        self.entry(key).map(|entry| entry.provider.chooses()).unwrap_or_default()
    }

    pub fn requires(&self, key: &ProviderKey) -> domain::data_policy::Requires {
        self.entry(key).map(|entry| entry.provider.requires()).unwrap_or_default()
    }

    pub fn descriptors(&self) -> Vec<&ProviderDescriptor> {
        self.entries.iter().map(|entry| &entry.descriptor).collect()
    }

    pub fn descriptor(&self, key: &ProviderKey) -> Result<&ProviderDescriptor, ActionError> {
        self.entry(key).map(|entry| &entry.descriptor)
    }

    pub fn presentation(&self, key: &ProviderKey, language: Language) -> Result<Presentation, ActionError> {
        self.entry(key).map(|entry| entry.provider.presentation(language))
    }

    pub fn alternatives(&self, key: &ProviderKey, language: Language) -> Result<Vec<AlternateAuth>, ActionError> {
        self.entry(key).map(|entry| entry.provider.alternatives(language))
    }

    pub fn http(&self) -> &dyn Http {
        self.http.as_ref()
    }

    pub fn keeper(&self) -> &TokenKeeper {
        &self.keeper
    }

    pub fn limiter(&self) -> &RateLimiter {
        &self.limiter
    }

    fn entry(&self, key: &ProviderKey) -> Result<&Entry, ActionError> {
        self.entries
            .iter()
            .find(|entry| &entry.descriptor.key == key)
            .ok_or_else(|| ActionError::permanent(format!("no provider is registered as `{key}`")))
    }

    async fn completed(&self, entry: &Entry, connection: &Connection, settings: &Value) -> Value {
        let known = self.learned.lock().unwrap_or_else(PoisonError::into_inner).get(&connection.id).cloned();
        let settings = known.map_or_else(|| settings.clone(), |known| filled(settings, &known));
        let chooses = entry.provider.chooses();
        if chooses.iter().all(|name| has(&settings, name)) {
            return settings;
        }
        let Ok(creds) = self.credentials(entry, connection, &settings).await else { return settings };
        let metered = Metered { inner: self.http.as_ref(), limiter: &self.limiter, connection: &connection.id };
        let options = entry.provider.accounts(&metered, &creds).await.unwrap_or_default();
        let [only] = options.as_slice() else { return settings };
        if !chooses.iter().all(|name| has(&only.settings, name)) {
            return settings;
        }
        let settings = filled(&settings, &only.settings);
        self.learned.lock().unwrap_or_else(PoisonError::into_inner).insert(connection.id.clone(), settings.clone());
        if let Some(sink) = &self.sink {
            sink.learned(&connection.id, &settings);
        }
        settings
    }

    async fn credentials(
        &self,
        entry: &Entry,
        connection: &Connection,
        settings: &Value,
    ) -> Result<Credentials, ActionError> {
        let token = self.keeper.access_token_of(&connection.id, &entry.descriptor.auth, &entry.alternatives).await?;
        Ok(Credentials::new(&connection.id, &token, settings.clone()))
    }

    pub async fn access_token(&self, connection: &Connection) -> Result<String, ActionError> {
        let entry = self.entry(&connection.provider)?;
        self.keeper.access_token_of(&connection.id, &entry.descriptor.auth, &entry.alternatives).await
    }

    pub async fn call(
        &self,
        connection: &Connection,
        settings: &Value,
        action: &str,
        args: &Value,
        idempotency_key: &str,
    ) -> Result<Value, ActionError> {
        let entry = self.entry(&connection.provider)?;
        let spec = entry.descriptor.action(action).ok_or_else(|| unknown_action(action))?;
        let capability = Capability::of_action(action).ok_or_else(|| unknown_action(action))?;
        if !connection.offers(&capability) {
            return Err(ActionError::permanent(format!(
                "the connection `{}` does not offer `{capability}`",
                connection.id
            )));
        }
        if let Some(missing) = spec.missing(args) {
            return Err(ActionError::missing(&missing));
        }
        let settings = self.completed(entry, connection, settings).await;
        let mut creds = self.credentials(entry, connection, &settings).await?;
        let metered = Metered { inner: self.http.as_ref(), limiter: &self.limiter, connection: &connection.id };
        let first = entry.provider.call(&metered, &creds, action, args, idempotency_key).await;
        let result = match first {
            Err(error)
                if error.class == ErrorClass::Auth && !error.message.starts_with(RECONNECT) && !error.needs_login() =>
            {
                creds.access_token = self
                    .keeper
                    .refreshed_of(&connection.id, &entry.descriptor.auth, &entry.alternatives, &creds.access_token)
                    .await?;
                entry.provider.call(&metered, &creds, action, args, idempotency_key).await
            }
            other => other,
        };
        if let Err(error) = &result {
            self.limiter.learn(&connection.id, error);
        }
        result
    }

    pub async fn poll(
        &self,
        connection: &Connection,
        settings: &Value,
        cursor: Option<&str>,
        watch: &[String],
    ) -> Result<Option<EventBatch>, ActionError> {
        let entry = self.entry(&connection.provider)?;
        let Some(source) = entry.provider.events() else { return Ok(None) };
        let settings = self.completed(entry, connection, settings).await;
        let creds = self.credentials(entry, connection, &settings).await?;
        let metered = Metered { inner: self.http.as_ref(), limiter: &self.limiter, connection: &connection.id };
        let mut batch = source
            .poll(&metered, &creds, cursor, watch)
            .await
            .inspect_err(|error| self.limiter.learn(&connection.id, error))?;
        batch.events.retain(|event| connection.offers(&event.capability));
        Ok(Some(batch))
    }

    pub async fn accounts(&self, connection: &Connection, settings: &Value) -> Result<Vec<AccountOption>, ActionError> {
        let entry = self.entry(&connection.provider)?;
        let creds = self.credentials(entry, connection, settings).await?;
        let metered = Metered { inner: self.http.as_ref(), limiter: &self.limiter, connection: &connection.id };
        entry.provider.accounts(&metered, &creds).await
    }
}

#[cfg(test)]
mod tests;
