use std::sync::{Arc, OnceLock};

use agent_engine::ApiSource;
use agent_engine::model::{BlockingTurn, TurnRequest};
use agent_engine::transcript::Message;
use domain::model_roles::ModelRole;
use domain::{Effort, PromptRequest, RunId};

use crate::BrowserAsk;
use crate::engine::ProviderApis;
use crate::features::context::{Context, Finish};

const ANSWER_TOKENS: u32 = 2000;

pub fn prepare(context: &Context<'_>, run: RunId, request: PromptRequest) -> (PromptRequest, Option<Finish>) {
    let session = request.session.resume.as_ref().map_or_else(|| run.to_string(), |resumed| resumed.0.clone());
    let roots = std::iter::once(request.session.cwd.clone()).chain(request.session.add_dirs.iter().cloned()).collect();
    let ask = asker(&request);
    let mut request = request;
    if let Some(server) = context.services.browser_mcp.for_run(&session, roots, ask) {
        request.host.credentials.mcp.push(server);
    }
    (request, None)
}

fn apis() -> &'static ProviderApis {
    static APIS: OnceLock<ProviderApis> = OnceLock::new();
    APIS.get_or_init(|| ProviderApis::new(Arc::new(agent_engine::model::http::NetHttp)))
}

fn asker(request: &PromptRequest) -> Option<BrowserAsk> {
    request.host.credentials.model.as_ref()?;
    let request = request.clone();
    Some(Arc::new(move |system: &str, prompt: &str| ask_small(&request, system, prompt)))
}

fn ask_small(request: &PromptRequest, system: &str, prompt: &str) -> Option<String> {
    let apis = apis();
    let api = apis.api(request).ok()?;
    let model = &request.session.model;
    let configured = request.session.roles.get(ModelRole::Small);
    let small = agent_engine::pick_small_model(model.provider, &model.id, configured, &apis.catalog(request));
    let turn = TurnRequest {
        model: small.model.unwrap_or_else(|| model.id.clone()),
        system: system.to_owned(),
        messages: vec![Message::user(prompt)],
        tools: Vec::new(),
        effort: Effort::Low,
        max_tokens: ANSWER_TOKENS,
        intent: agent_engine::model::Intent::ONCE,
    };
    let stop = platform::Stop::new();
    match api.turn_blocking(&turn, &mut |_| {}, stop.token()) {
        Ok(reply) => Some(reply.message.text()),
        Err(error) => {
            tracing::warn!(%error, "the browser's find could not ask a model; it ranks the elements itself");
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::*;
    use crate::BrowserMcp;
    use crate::features::context::testing;

    fn request() -> PromptRequest {
        PromptRequest::plain("/repo".into(), "Open the site", domain::ModelOption::new("opus", "Opus"))
    }

    #[test]
    fn every_run_gets_the_browser_server_for_its_conversation_and_folders() {
        let asked = Arc::new(Mutex::new(Vec::new()));
        let seen = Arc::clone(&asked);
        let browser = BrowserMcp::new(move |session, roots, _ask| {
            seen.lock().unwrap_or_else(std::sync::PoisonError::into_inner).push((session.to_owned(), roots));
            Some(domain::mcp::LocalMcp::new("browser", Arc::new(Silent)))
        });
        let services = testing::services(None).with_browser_mcp(browser);
        let context = Context { services: &services };
        let mut request = request();
        request.session.resume = Some(domain::SessionId("conversation".into()));
        let (prepared, finish) = prepare(&context, RunId::new(), request.clone());
        assert!(finish.is_none());
        assert_eq!(prepared.host.credentials.mcp.iter().map(|mcp| mcp.name.as_str()).collect::<Vec<_>>(), ["browser"]);
        let asked = asked.lock().unwrap_or_else(std::sync::PoisonError::into_inner).clone();
        assert_eq!(asked, [("conversation".to_owned(), vec![request.session.cwd.clone()])]);
    }

    #[test]
    fn a_turned_off_browser_adds_no_server_to_the_run() {
        let services = testing::services(None).with_browser_mcp(BrowserMcp::new(|_, _, _| None));
        let context = Context { services: &services };
        let (prepared, _) = prepare(&context, RunId::new(), request());
        assert!(prepared.host.credentials.mcp.is_empty());
    }

    #[test]
    fn a_run_without_a_model_key_gets_no_model_for_find() {
        assert!(asker(&request()).is_none());
        let mut keyed = request();
        keyed.host.credentials.model = Some(domain::provider::ProviderKey::new("k"));
        assert!(asker(&keyed).is_some());
    }

    #[test]
    fn without_a_browser_server_nothing_is_added() {
        let services = testing::services(None);
        let context = Context { services: &services };
        let (prepared, _) = prepare(&context, RunId::new(), request());
        assert!(prepared.host.credentials.mcp.is_empty());
    }

    struct Silent;

    impl domain::mcp::McpEndpoint for Silent {
        fn handle(&self, _message: &serde_json::Value) -> Option<serde_json::Value> {
            None
        }
    }
}
