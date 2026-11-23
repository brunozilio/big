use std::sync::Weak;

use async_trait::async_trait;
use connectors::ExternalEvent;
use domain::workflow::{ItemId, WorkflowEvent};
use sync::{Error, EventSink};

use crate::api::Orchestrator;
use crate::shared::Shared;

#[derive(Debug)]
pub struct KernelSink {
    shared: Weak<Shared>,
}

impl KernelSink {
    pub fn new(shared: Weak<Shared>) -> KernelSink {
        KernelSink { shared }
    }

    fn orchestrator(&self) -> Result<Orchestrator, Error> {
        self.shared.upgrade().map(Orchestrator::of).ok_or_else(|| Error::Sink("the kernel has stopped".into()))
    }
}

fn failed(error: crate::error::Error) -> Error {
    Error::Sink(error.to_string())
}

#[async_trait]
impl EventSink for KernelSink {
    async fn deliver(&self, event: &ExternalEvent) -> Result<usize, Error> {
        self.orchestrator()?.deliver_event(event).map_err(failed)
    }

    async fn trigger(&self, event: &ExternalEvent) -> Result<usize, Error> {
        let orchestrator = self.orchestrator()?;
        let event = event.clone();
        tokio::task::spawn_blocking(move || orchestrator.trigger(&event))
            .await
            .map_err(|error| Error::Sink(error.to_string()))?
            .map_err(failed)
    }

    async fn record(&self, item: &ItemId, events: &[WorkflowEvent]) -> Result<(), Error> {
        self.orchestrator()?.record(item, events).map_err(failed)
    }
}
