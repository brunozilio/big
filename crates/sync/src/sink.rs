use async_trait::async_trait;
use connectors::ExternalEvent;
use domain::workflow::{ItemId, WorkflowEvent};

use crate::error::Error;

#[async_trait]
pub trait EventSink: Send + Sync + std::fmt::Debug {
    async fn deliver(&self, event: &ExternalEvent) -> Result<usize, Error>;

    async fn trigger(&self, event: &ExternalEvent) -> Result<usize, Error>;

    async fn record(&self, item: &ItemId, events: &[WorkflowEvent]) -> Result<(), Error>;
}
