use std::sync::Arc;

use domain::connection::{Capability, Connection, ProviderKey};
use serde_json::Value;

use crate::descriptor::{AlternateAuth, Presentation, ProviderDescriptor};
use crate::error::{ActionError, ErrorClass, RECONNECT};
use crate::http::Http;
use crate::limiter::{Metered, RateLimiter};
use crate::oauth::TokenKeeper;
use crate::provider::{AccountOption, Credentials, EventBatch, Provider, unknown_action};
use crate::providers;

#[derive(Debug)]
struct Entry {
    provider: &'static dyn Provider,
    descriptor: ProviderDescriptor,
    presentation: Presentation,
    alternative: Option<AlternateAuth>,
}

#[derive(Debug)]
pub struct Connectors {
    entries: Vec<Entry>,
    http: Arc<dyn Http>,
    keeper: TokenKeeper,
    limiter: RateLimiter,
}

impl Connectors {
    pub fn new(http: Arc<dyn Http>, keeper: TokenKeeper) -> Connectors {
        Connectors::with_providers(providers::ALL, http, keeper)
    }

    pub fn with_providers(providers: &[&'static dyn Provider], http: Arc<dyn Http>, keeper: TokenKeeper) -> Connectors {
        let entries = providers
            .iter()
            .map(|provider| Entry {
                provider: *provider,
                descriptor: provider.descriptor(),
                presentation: provider.presentation(),
                alternative: provider.alternative(),
            })
            .collect();
        Connectors { entries, http, keeper, limiter: RateLimiter::new() }
    }

    pub fn descriptors(&self) -> Vec<&ProviderDescriptor> {
        self.entries.iter().map(|entry| &entry.descriptor).collect()
    }

    pub fn descriptor(&self, key: &ProviderKey) -> Result<&ProviderDescriptor, ActionError> {
        self.entry(key).map(|entry| &entry.descriptor)
    }

    pub fn presentation(&self, key: &ProviderKey) -> Result<&Presentation, ActionError> {
        self.entry(key).map(|entry| &entry.presentation)
    }

    pub fn alternative(&self, key: &ProviderKey) -> Result<Option<&AlternateAuth>, ActionError> {
        self.entry(key).map(|entry| entry.alternative.as_ref())
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

    async fn credentials(
        &self,
        entry: &Entry,
        connection: &Connection,
        settings: &Value,
    ) -> Result<Credentials, ActionError> {
        let token =
            self.keeper.access_token_of(&connection.id, &entry.descriptor.auth, entry.alternative.as_ref()).await?;
        Ok(Credentials::new(&connection.id, &token, settings.clone()))
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
        let mut creds = self.credentials(entry, connection, settings).await?;
        let metered = Metered { inner: self.http.as_ref(), limiter: &self.limiter, connection: &connection.id };
        let first = entry.provider.call(&metered, &creds, action, args, idempotency_key).await;
        let result = match first {
            Err(error) if error.class == ErrorClass::Auth && !error.message.starts_with(RECONNECT) => {
                let alternative = entry.alternative.as_ref();
                creds.access_token = self
                    .keeper
                    .refreshed_of(&connection.id, &entry.descriptor.auth, alternative, &creds.access_token)
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
    ) -> Result<Option<EventBatch>, ActionError> {
        let entry = self.entry(&connection.provider)?;
        let Some(source) = entry.provider.events() else { return Ok(None) };
        let creds = self.credentials(entry, connection, settings).await?;
        let metered = Metered { inner: self.http.as_ref(), limiter: &self.limiter, connection: &connection.id };
        let mut batch = source
            .poll(&metered, &creds, cursor)
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
