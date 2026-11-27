use serde_json::Value;

use crate::tools::Workspace;
use crate::tools::subagent_config::AgentConfig;

pub struct Choice {
    pub model: Option<String>,
    pub unserved: Option<String>,
}

pub fn choose(kind: &str, configured: Option<String>, input: &Value, parent: &str, workspace: &Workspace) -> Choice {
    let provider = workspace.models.model.provider;
    let served = |model: Option<String>| {
        let config = AgentConfig { model, ..Default::default() };
        config.model.is_some() && config.model(parent, provider).is_ok()
    };
    let configured = configured.and_then(|named| match workspace.models.tiers.find(&named) {
        Some(_) => {
            Some(workspace.models.resolved(&named, parent).to_owned()).filter(|model| served(Some(model.clone())))
        }
        None => Some(named),
    });
    let mut choice = Choice { model: configured, unserved: None };
    let asked = input.get("model").and_then(Value::as_str).filter(|m| !m.is_empty());
    if let Some(asked) = asked.map(|named| workspace.models.resolved(named, parent)) {
        if served(Some(asked.to_owned())) {
            choice.model = Some(asked.to_owned());
        } else {
            choice.unserved = Some(asked.to_owned());
        }
    }
    if choice.model.is_some() {
        return choice;
    }
    let models = &workspace.models;
    let role = models.roles.for_agent(kind, models.small_model.as_deref()).map(str::to_owned);
    if served(role.clone()) {
        choice.model = role;
    }
    choice
}
