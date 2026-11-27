use domain::code_mode::{ADAPT_SCRIPT, LOAD_SCRIPT, SEARCH_SCRIPTS};
use serde_json::Value;

use super::Agent;
use crate::model::{BlockingTurn, TurnRequest};
use crate::tools::bank;
use crate::tools::names::REPL;
use crate::tools::repl::reachable;
use crate::transcript::Message;

const ADAPTED_TOKENS: u32 = 16_000;

impl Agent<'_> {
    pub(super) fn answer_from_bank(&self, function: &str, input: &Value) -> Option<Result<Value, String>> {
        let has = |tool: &str| self.has_tool(tool);
        match function {
            SEARCH_SCRIPTS => Some(match input["query"].as_str().map(str::trim).filter(|query| !query.is_empty()) {
                Some(query) => bank::search(query, REPL, has),
                None => Err("searchScripts needs the words to look for, e.g. searchScripts('changed files')".into()),
            }),
            LOAD_SCRIPT => Some(match input["source"].as_str() {
                Some(source) => bank::compile(source, REPL, has),
                None => bank::load(input["name"].as_str().unwrap_or_default(), REPL, has),
            }),
            ADAPT_SCRIPT => {
                let tools: Vec<String> =
                    self.request.tools.iter().map(|spec| spec.name.clone()).filter(|name| reachable(name)).collect();
                Some(bank::adapt(input, &tools, |system, prompt| self.ask_small_model(system, prompt)))
            }
            _ => None,
        }
    }

    fn has_tool(&self, name: &str) -> bool {
        let offered = self.request.tools.iter().any(|spec| spec.name == name);
        let deferred = self.workspace.services.deferred.as_ref();
        reachable(name) && (offered || deferred.is_some_and(|tools| tools.specs().iter().any(|spec| spec.name == name)))
    }

    fn ask_small_model(&self, system: String, prompt: String) -> Result<String, String> {
        let models = &self.workspace.models;
        let small = crate::tools::builtin::web::guarded::reader(
            models.small_model.as_deref(),
            &self.request,
            models.model.provider,
        );
        let request = TurnRequest {
            system,
            messages: vec![Message::user(prompt)],
            tools: Vec::new(),
            effort: domain::Effort::Low,
            max_tokens: small.max_tokens.min(ADAPTED_TOKENS),
            ..small
        };
        let reply = self.api.turn_blocking(&request, &mut |_| {}, self.control.token());
        reply.map(|reply| reply.message.text()).map_err(|error| error.to_string())
    }
}
