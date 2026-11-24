use std::sync::{Arc, Mutex};

use agent_engine::{AgentEngine, EngineError, EventSink, RunHandle};
use domain::memory::MemoryBrief;
use domain::{EngineEvent, ModelOption, PromptRequest, RunId};
use platform::sync::Locked;

use crate::catalog::Listing;
use crate::services::LocalServices;

pub struct KeyedEngine {
    services: LocalServices,
    known: Mutex<Vec<ModelOption>>,
}

impl std::fmt::Debug for KeyedEngine {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_struct("KeyedEngine").finish_non_exhaustive()
    }
}

impl KeyedEngine {
    pub fn new(services: LocalServices) -> KeyedEngine {
        KeyedEngine { services, known: Mutex::default() }
    }

    pub fn remember(&self, models: Vec<ModelOption>) {
        *self.known.locked() = models;
    }
}

struct Listed {
    listing: Option<Listing>,
    ending: crate::prepare::Ending,
    inner: Arc<dyn EventSink>,
}

impl EventSink for Listed {
    fn emit(&self, run: RunId, event: EngineEvent) {
        if let Some(listing) = &self.listing {
            listing.observe(&event);
        }
        for finished in self.ending.observe(&event) {
            tracing::debug!(%run, ?finished, "an agent run finished its features");
        }
        if event.is_terminal() {
            agent_engine::tools::peers::forget(self.ending.client());
        }
        self.inner.emit(run, event);
    }
}

impl AgentEngine for KeyedEngine {
    fn models(&self) -> Vec<ModelOption> {
        let known = self.known.locked().clone();
        if known.is_empty() { self.services.engine.models() } else { known }
    }

    fn start(
        &self,
        run: RunId,
        request: &PromptRequest,
        sink: Arc<dyn EventSink>,
    ) -> Result<Box<dyn RunHandle>, EngineError> {
        let mut request = request.clone();
        if request.host.memory.is_none() {
            request.host.memory = Some(MemoryBrief::new(None));
        }
        let client = request.host.credentials.client;
        let prepared = crate::prepare::prepare(&self.services, client, run, &request);
        let listing = Listing::of(&self.services, &prepared.request);
        let sink = Arc::new(Listed { listing, ending: prepared.ending, inner: sink });
        self.services.engine.start(run, &prepared.request, sink)
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use agent_engine::fake::FakeEngine;

    use super::*;

    #[test]
    fn a_run_carries_the_connected_key_and_is_listed_in_the_catalog() {
        llm_providers::testing::install();
        let model = ModelOption::new("m", "M").served_by(domain::ModelProvider::new("keyed"));
        let engine = FakeEngine::new(vec![model.clone()], Vec::new());
        let services = LocalServices::with_engine(Arc::new(engine.clone()));
        services.providers.connect(domain::ModelProvider::new("keyed"), "sk-proj-0123456789abcd");
        let keyed = KeyedEngine::new(services.clone());
        keyed.remember(vec![model.clone()]);
        assert_eq!(keyed.models(), vec![model.clone()]);
        let mut request = PromptRequest::plain(Path::new("/work").to_path_buf(), "oi", model);
        request.host.credentials.client = 7;
        let sink: Arc<dyn EventSink> = Arc::new(|_: RunId, _: EngineEvent| {});
        let started = keyed.start(RunId::new(), &request, sink);
        assert!(started.is_ok());
        let sent = engine.requests();
        assert!(sent.iter().any(|sent| sent.host.credentials.model.is_some()), "the key went with the run");
        assert!(sent.iter().all(|sent| sent.host.credentials.client == 7), "the run keeps the client it was given");
        assert!(sent.iter().all(|sent| sent.host.credentials.mcp_tokens.is_some()), "the run carries its MCP tokens");
    }
}
