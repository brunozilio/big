use std::sync::Mutex;

use domain::memory::scorecard::RunOutcome;
use domain::{EngineEvent, PromptRequest, RunId};
use platform::sync::Locked;

use crate::api::Unsolicited;
use crate::features::context::{Context, Finish};
use crate::features::trace::export::Watch;
use crate::services::LocalServices;

pub(crate) struct Prepared {
    pub request: PromptRequest,
    pub ending: Ending,
}

pub(crate) struct Ending {
    client: u64,
    finishes: Mutex<Vec<Finish>>,
    outcome: Mutex<RunOutcome>,
    exported: Option<Watch>,
    learning: Option<crate::features::memory::lesson::Learning>,
}

impl Ending {
    pub fn observe(&self, event: &EngineEvent) -> Vec<Unsolicited> {
        if let Some(exported) = &self.exported {
            exported.observe(event);
        }
        self.outcome.locked().observe(event);
        if let Some(learning) = &self.learning {
            learning.observe(event);
        }
        if !event.is_terminal() {
            return Vec::new();
        }
        let outcome = *self.outcome.locked();
        let finishes = std::mem::take(&mut *self.finishes.locked());
        finishes.into_iter().filter_map(|finish| finish(&outcome)).collect()
    }

    pub fn client(&self) -> u64 {
        self.client
    }
}

pub(crate) fn prepare(services: &LocalServices, client: u64, run: RunId, request: &PromptRequest) -> Prepared {
    agent_engine::tools::peers::register(client, &request.session.cwd);
    let providers = &services.providers;
    let (model_accounts, mut keys): (Vec<String>, Vec<_>) =
        crate::engine::keys_for(providers, &request.session.model).into_iter().unzip();
    let mut request = request.clone();
    let credentials = &mut request.host.credentials;
    credentials.model = (!keys.is_empty()).then(|| keys.remove(0));
    credentials.model_pool = keys;
    credentials.model_accounts = model_accounts;
    credentials.other_models = crate::engine::other_keys(providers, &request.session.model);
    credentials.client = client;
    credentials.mcp_tokens = Some(services.mcp_auth.for_run());
    credentials.remote_mcp = services.connected_mcp.servers(request.host.owner.as_deref());
    let request = services.extensions.brief(request, services.home());
    let context = Context { services };
    let (request, finishes) = crate::features::prepare(&context, run, request);
    let exported = services
        .exporter
        .as_ref()
        .map(|exporter| exporter.watch(&request.session.cwd, request.session.resume.as_ref()));
    let learning = crate::features::memory::lesson::Learning::of(services, &request);
    let ending = Ending { client, finishes: Mutex::new(finishes), outcome: Mutex::default(), exported, learning };
    Prepared { request, ending }
}
