use std::path::PathBuf;

use serde_json::Value;

use super::script_tool::{Capabilities, Exports, ScriptTool};
use super::{Declared, declared};

pub fn path(name: &str) -> String {
    format!("tools/conditional/{name}.ts")
}

pub fn tool(name: &str, data: &Value) -> Option<ScriptTool> {
    let path = path(name);
    let Some(source) = content::source(&path) else {
        tracing::error!(name, "a tool offered only in some sessions is not shipped");
        return None;
    };
    let shape = content::value(&path, "spec").unwrap_or_default();
    let capabilities = Capabilities::read(&shape)
        .inspect_err(|error| tracing::error!(%error, name, "a tool offered only in some sessions does not load"))
        .ok()?;
    let has = |export: &str| script::exports(&path, &source, export);
    let exports =
        Exports { effects: has("effects"), summary: has("summary"), shown: has("shown"), offered: has("offered") };
    Some(ScriptTool {
        spec: declared(name, data),
        declared: Some(Declared::Executes),
        capabilities,
        exports,
        serial: shape["serial"].as_bool() == Some(true),
        source,
        path: PathBuf::from(path),
        trusted: true,
        skills: Vec::new(),
        interceptors: Vec::new(),
    })
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::tools::{Tool, Workspace};

    #[test]
    fn a_tool_offered_only_in_some_sessions_is_built_from_its_script_with_the_session_s_data() {
        let listed = tool("ListMcpResources", &json!({ "servers": ["docs", "wiki"] })).expect("shipped");
        assert!(listed.spec.description.contains("docs, wiki"), "{}", listed.spec.description);
        assert_eq!(listed.spec.schema["required"], json!(["server"]));
        assert!(listed.trusted && listed.exports.effects && listed.exports.summary);
        assert_eq!(listed.summary(&json!({ "server": "docs" })), "docs");
        let workspace = Workspace::new(".");
        let effects = listed.effects(&json!({ "server": "docs" }), &workspace);
        assert_eq!(effects, [crate::tools::Effect::Resource { server: "docs".into(), uri: String::new() }]);
        assert!(tool("AddRepository", &Value::Null).is_some_and(|added| added.serial));
        assert!(tool("Nothing", &Value::Null).is_none());
        assert!(!crate::tools::standard().iter().any(|tool| tool.spec().name == "ObserverReport"));
    }
}
