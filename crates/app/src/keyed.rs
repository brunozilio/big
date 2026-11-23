use std::sync::{Arc, Mutex};

use agent_engine::{AgentEngine, EngineError, EventSink, RunHandle};
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

    fn keyed(&self, request: &PromptRequest) -> PromptRequest {
        let providers = &self.services.providers;
        let (model_accounts, mut keys): (Vec<String>, Vec<_>) =
            crate::engine::keys_for(providers, &request.session.model).into_iter().unzip();
        let mut request = request.clone();
        request.host.credentials.model = (!keys.is_empty()).then(|| keys.remove(0));
        request.host.credentials.model_pool = keys;
        request.host.credentials.model_accounts = model_accounts;
        request.host.credentials.other_models = crate::engine::other_keys(providers, &request.session.model);
        request.host.credentials.mcp_tokens = Some(self.services.mcp_auth.for_run());
        request
    }
}

struct Listed {
    listing: Option<Listing>,
    inner: Arc<dyn EventSink>,
}

impl EventSink for Listed {
    fn emit(&self, run: RunId, event: EngineEvent) {
        if let Some(listing) = &self.listing {
            listing.observe(&event);
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
        let request = self.keyed(request);
        let listing = Listing::of(&self.services, &request);
        self.services.engine.start(run, &request, Arc::new(Listed { listing, inner: sink }))
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
        let request = PromptRequest::plain(Path::new("/work").to_path_buf(), "oi", model);
        let sink: Arc<dyn EventSink> = Arc::new(|_: RunId, _: EngineEvent| {});
        let started = keyed.start(RunId::new(), &request, sink);
        assert!(started.is_ok());
        let sent = engine.requests();
        assert!(sent.iter().any(|sent| sent.host.credentials.model.is_some()), "the key went with the run");
    }
}
