mod edit;
mod listing;

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub use self::edit::{McpOrigin, McpServerInfo, McpState};
pub use self::listing::{McpListing, McpUnreadable};

pub const RESERVED_PREFIXES: [&str; 1] = [concat!(brand::command!(), "_")];

pub fn is_free_name(name: &str) -> bool {
    !RESERVED_PREFIXES.iter().any(|prefix| name.starts_with(prefix))
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpApproval {
    pub disabled: Vec<String>,
}

impl McpApproval {
    pub fn read(&mut self, settings: &Value) {
        let names = settings["disabledMcpServers"].as_array().into_iter().flatten();
        self.disabled.extend(names.filter_map(|name| name.as_str().map(str::to_owned)));
    }

    pub fn allows(&self, name: &str) -> bool {
        !self.disabled.iter().any(|known| known == name)
    }
}

pub fn expand(value: Value, lookup: &dyn Fn(&str) -> Option<String>) -> Value {
    match value {
        Value::String(text) => Value::String(expand_text(&text, lookup)),
        Value::Array(items) => Value::Array(items.into_iter().map(|item| expand(item, lookup)).collect()),
        Value::Object(map) => Value::Object(map.into_iter().map(|(key, item)| (key, expand(item, lookup))).collect()),
        other => other,
    }
}

fn expand_text(text: &str, lookup: &dyn Fn(&str) -> Option<String>) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find("${") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        let Some(end) = after.find('}') else {
            out.push_str(&rest[start..]);
            return out;
        };
        let inner = &after[..end];
        let (name, default) = match inner.split_once(":-") {
            Some((name, default)) => (name, Some(default)),
            None => (inner, None),
        };
        match lookup(name).or_else(|| default.map(str::to_owned)) {
            Some(found) => out.push_str(&found),
            None => out.push_str(&rest[start..start + 2 + end + 1]),
        }
        rest = &after[end + 1..];
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn a_project_server_starts_unless_it_is_turned_off() {
        let mut approval = McpApproval::default();
        assert!(approval.allows("jira"), "the project's mcp.json is the configuration");
        approval.read(&json!({"disabledMcpServers": ["jira"]}));
        assert!(!approval.allows("jira") && approval.allows("sonar"));
    }

    #[test]
    fn variables_are_filled_in_with_defaults() {
        let lookup = |name: &str| (name == "TOKEN").then(|| "s3cret".to_owned());
        let config = json!({"headers": {"Authorization": "Bearer ${TOKEN}"},
            "args": ["${HOST:-sonar.local}", "${MISSING}", "${broken"], "port": 9});
        assert_eq!(
            expand(config, &lookup),
            json!({"headers": {"Authorization": "Bearer s3cret"},
                "args": ["sonar.local", "${MISSING}", "${broken"], "port": 9})
        );
    }

    #[test]
    fn a_server_s_state_says_why_it_does_or_does_not_start() {
        let mut approval = McpApproval::default();
        approval.read(&json!({"disabledMcpServers": ["sonar"]}));
        assert_eq!(McpState::of(&McpOrigin::Project, "jira", &approval), McpState::Ready, "a project's mcp.json counts");
        assert_eq!(McpState::of(&McpOrigin::Project, "sonar", &approval), McpState::Off);
        assert_eq!(McpState::of(&McpOrigin::User, "new", &approval), McpState::Ready);
    }

    #[test]
    fn the_platform_s_prefixes_are_not_free() {
        assert!(is_free_name("jira"));
        assert!(!is_free_name(concat!(brand::command!(), "_memory")));
    }
}
