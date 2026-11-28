use std::collections::BTreeMap;

use serde::Deserialize;
use serde_json::Value;

use super::MemoryScope;
use crate::permission::{MemoryAccess, Surface};

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub struct MemoryRules {
    #[serde(default)]
    pub read: Option<Vec<String>>,
    #[serde(default)]
    pub write: Option<BTreeMap<String, Vec<MemoryScope>>>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub struct MemoryPolicy {
    #[serde(flatten)]
    pub base: MemoryRules,
    #[serde(default)]
    pub applications: BTreeMap<String, MemoryRules>,
}

impl MemoryPolicy {
    pub fn of(value: Option<Value>) -> Self {
        let Some(value) = value else { return Self::default() };
        serde_json::from_value(value).unwrap_or_else(|_| Self::refusing())
    }

    pub fn refusing() -> Self {
        Self { base: MemoryRules { read: Some(Vec::new()), write: Some(BTreeMap::new()) }, ..Self::default() }
    }

    fn rules(&self, application: Option<&str>) -> MemoryRules {
        let named = application.and_then(|name| {
            self.applications
                .iter()
                .find(|(key, _)| key.trim().eq_ignore_ascii_case(name.trim()))
                .map(|(_, rules)| rules)
        });
        match named {
            Some(own) => MemoryRules {
                read: own.read.clone().or_else(|| self.base.read.clone()),
                write: own.write.clone().or_else(|| self.base.write.clone()),
            },
            None => self.base.clone(),
        }
    }

    pub fn refusal(&self, surface: Surface, access: MemoryAccess, application: Option<&str>) -> Option<String> {
        let rules = self.rules(application);
        let word = surface.word();
        match access {
            MemoryAccess::Read => {
                let read = rules.read?;
                (!read.iter().any(|named| named.trim() == word))
                    .then(|| format!("the memory policy does not let {word} read memory."))
            }
            MemoryAccess::Write(scope) => {
                let write = rules.write?;
                let scopes = write.get(word).map(Vec::as_slice).unwrap_or_default();
                (!scopes.contains(&scope)).then(|| {
                    format!("the memory policy does not let {word} write memory at the {} scope.", scope_word(scope))
                })
            }
        }
    }
}

fn scope_word(scope: MemoryScope) -> &'static str {
    match scope {
        MemoryScope::Repository => "repository",
        MemoryScope::Application => "application",
        MemoryScope::Global => "global",
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn nothing_declared_keeps_every_surface_reading_and_writing() {
        let policy = MemoryPolicy::of(None);
        for surface in Surface::ALL {
            assert_eq!(policy.refusal(surface, MemoryAccess::Read, None), None);
            assert_eq!(policy.refusal(surface, MemoryAccess::Write(MemoryScope::Global), None), None);
        }
    }

    #[test]
    fn a_declared_write_names_the_scopes_each_surface_keeps() {
        let policy =
            MemoryPolicy::of(Some(json!({ "write": { "chat": ["repository", "global"], "mcp": ["repository"] } })));
        assert_eq!(policy.refusal(Surface::Mcp, MemoryAccess::Write(MemoryScope::Repository), None), None);
        let refused = policy.refusal(Surface::Mcp, MemoryAccess::Write(MemoryScope::Global), None);
        assert_eq!(refused.as_deref(), Some("the memory policy does not let mcp write memory at the global scope."));
        assert!(policy.refusal(Surface::Workflow, MemoryAccess::Write(MemoryScope::Repository), None).is_some());
        assert_eq!(policy.refusal(Surface::Workflow, MemoryAccess::Read, None), None);
    }

    #[test]
    fn an_application_overrides_only_what_it_declares() {
        let policy = MemoryPolicy::of(Some(json!({
            "read": ["chat", "mcp"],
            "applications": { "Rocket": { "write": { "chat": ["application"] } } },
        })));
        let rocket = Some("rocket");
        assert!(policy.refusal(Surface::Chat, MemoryAccess::Write(MemoryScope::Repository), rocket).is_some());
        assert_eq!(policy.refusal(Surface::Chat, MemoryAccess::Write(MemoryScope::Application), rocket), None);
        assert!(policy.refusal(Surface::Workflow, MemoryAccess::Read, rocket).is_some());
        assert_eq!(policy.refusal(Surface::Chat, MemoryAccess::Write(MemoryScope::Global), Some("other")), None);
    }

    #[test]
    fn a_policy_that_cannot_be_read_refuses_instead_of_opening() {
        let policy = MemoryPolicy::of(Some(json!({ "write": "everyone" })));
        assert!(policy.refusal(Surface::Chat, MemoryAccess::Read, None).is_some());
        assert!(policy.refusal(Surface::Chat, MemoryAccess::Write(MemoryScope::Repository), None).is_some());
    }
}
