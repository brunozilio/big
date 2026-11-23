use domain::effect::{Effect, EffectOutcome};
use domain::workflow::WorkItem;
use serde::Serialize;
use serde_json::Value;

use crate::resources::Resources;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AgentCard {
    pub id: String,
    pub name: String,
    pub shape: Option<String>,
    pub color: Option<String>,
}

fn after<'a>(text: &'a str, key: &str) -> Option<&'a str> {
    let start = text.find(key)? + key.len();
    let value = text[start..].split_whitespace().next()?;
    Some(value.trim_matches(|letter| letter == '"' || letter == '\'')).filter(|value| !value.is_empty())
}

pub fn card(resources: &Resources, name: &str) -> AgentCard {
    let text = resources.agent(name).map(|found| found.text).unwrap_or_default();
    let (header, _) = agent_engine::frontmatter::parse(&text);
    let avatar = header.get("avatar").unwrap_or_default();
    AgentCard {
        id: name.to_owned(),
        name: header.get("label").or_else(|| header.get("name")).unwrap_or(name).to_owned(),
        shape: after(avatar, "shape:").map(str::to_owned),
        color: after(avatar, "color:").map(str::to_owned),
    }
}

fn session_of(progress: &Value, outcome: Option<&EffectOutcome>) -> Option<String> {
    let done = match outcome {
        Some(EffectOutcome::Ok { value }) => value["session"].as_str(),
        _ => None,
    };
    done.or_else(|| progress["session"].as_str()).map(str::to_owned)
}

pub fn agents_of(items: &[WorkItem], session: &str) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    for entry in items.iter().flat_map(|item| item.effects.values()) {
        let Effect::AgentRun { agent, .. } = &entry.request.effect else { continue };
        if session_of(&entry.progress, entry.outcome.as_ref()).as_deref() == Some(session) && !names.contains(agent) {
            names.push(agent.clone());
        }
    }
    names
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;

    #[test]
    fn a_card_reads_the_label_and_the_avatar_of_the_frontmatter() {
        let text = "---\nname: dev\nlabel: Desenvolvedor\navatar:\n  shape: squircle\n  color: \"#3b82f6\"\n---\ncorpo";
        let builtin: crate::Builtin = Arc::new(move |path: &str| (path == "agents/dev.md").then(|| text.to_owned()));
        let resources = Resources::new(None, None, builtin);
        let found = card(&resources, "dev");
        assert_eq!(found.name, "Desenvolvedor");
        assert_eq!(found.shape.as_deref(), Some("squircle"));
        assert_eq!(found.color.as_deref(), Some("#3b82f6"));
        let missing = card(&resources, "x");
        assert_eq!((missing.name.as_str(), missing.shape), ("x", None));
    }
}
