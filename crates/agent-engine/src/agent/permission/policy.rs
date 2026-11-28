use domain::permission::{Call, Effect, Judgement, PolicyContext, PolicyEngine, Surface, Verdict};
use serde_json::Value;

use crate::agent::Agent;

pub(in crate::agent) struct Called<'r> {
    pub(in crate::agent) name: &'r str,
    pub(in crate::agent) subject: &'r str,
    pub(in crate::agent) input: &'r Value,
    pub(in crate::agent) effects: &'r [Effect],
}

impl Called<'_> {
    pub(in crate::agent) fn call(&self) -> Call<'_> {
        Call { tool: self.name, subject: self.subject, effects: self.effects }
    }
}

impl Agent<'_> {
    pub(in crate::agent) fn judged(&self, called: &Called<'_>) -> Judgement {
        let remembered = self.control.remembered();
        let bounds = self.workspace.bounds();
        let allowed = domain::permission::allowed::global();
        let context = PolicyContext {
            mode: self.control.mode(),
            ceiling: self.control.ceiling(),
            remembered: &remembered,
            bounds: &bounds,
            no_prompts: self.control.no_prompts(),
            allowed: allowed.as_ref(),
        };
        let judgement = PolicyEngine::judge(&called.call(), &context);
        if let Verdict::Deny(reason) = &judgement.verdict {
            let stage = self.workspace.session_id.as_str();
            let owner = self.workspace.owner.as_deref().unwrap_or(domain::principal::LOCAL_USER);
            let asked = crate::policy_gate::Asked { surface: Surface::Chat, stage, owner, operation: None };
            crate::policy_gate::blocked(asked, called.name, reason, PolicyEngine::layer(&called.call(), &context));
        }
        judgement
    }
}
