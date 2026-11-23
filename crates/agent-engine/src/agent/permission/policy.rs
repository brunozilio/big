use domain::permission::{Call, Effect, Judgement, PolicyContext, PolicyEngine};
use serde_json::Value;

use crate::agent::Agent;

pub(in crate::agent) struct Called<'r> {
    pub(in crate::agent) name: &'r str,
    pub(in crate::agent) subject: &'r str,
    pub(in crate::agent) input: &'r Value,
    pub(in crate::agent) effects: &'r [Effect],
}

impl Called<'_> {
    fn call(&self) -> Call<'_> {
        Call { tool: self.name, subject: self.subject, effects: self.effects }
    }
}

impl Agent<'_> {
    pub(in crate::agent) fn judged(&self, called: &Called<'_>) -> Judgement {
        let remembered = self.control.remembered();
        let bounds = self.workspace.bounds();
        let context = PolicyContext {
            mode: self.control.mode(),
            ceiling: self.control.ceiling(),
            remembered: &remembered,
            bounds: &bounds,
            no_prompts: self.control.no_prompts(),
        };
        PolicyEngine::judge(&called.call(), &context)
    }
}
