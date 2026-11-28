use std::sync::Arc;

use async_trait::async_trait;
use connectors::audit_sink::{Ack, CAPABILITY};
use connectors::{ActionError, Connectors};
use domain::connection::{Capability, ProviderKey};
use serde_json::Value;
use store::Store;

use crate::calls::connection;

#[async_trait]
pub trait Sink: Send + Sync + std::fmt::Debug {
    fn name(&self) -> &str;

    async fn deliver(&self, events: &[Value], idempotency_key: &str) -> Result<Ack, ActionError>;
}

#[derive(Debug)]
pub struct ConnectorSink {
    pub key: ProviderKey,
    pub store: Arc<Store>,
    pub connectors: Arc<Connectors>,
}

#[async_trait]
impl Sink for ConnectorSink {
    fn name(&self) -> &str {
        self.key.as_str()
    }

    async fn deliver(&self, events: &[Value], idempotency_key: &str) -> Result<Ack, ActionError> {
        let records = self.store.connections().map_err(|error| ActionError::transient(error.to_string()))?;
        let capability = Capability::from(CAPABILITY);
        let bound = records
            .iter()
            .map(|record| (connection(record), record))
            .find(|(found, _)| found.provider == self.key && found.offers(&capability));
        let bound = bound.as_ref().map(|(found, record)| (found, &record.settings));
        self.connectors.deliver_audit(&self.key, bound, events, idempotency_key).await
    }
}
