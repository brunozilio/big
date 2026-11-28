pub mod after;
pub mod agent_file;
pub mod bank;
pub mod bank_run;
pub mod builtin;
pub mod builtin_agents;
#[cfg(test)]
mod builtin_skills;
pub mod checked;
pub mod clients;
pub mod conditional;
pub mod contract;
mod hard_floor;
pub mod host;
pub mod hosted;
pub mod inbox;
pub mod names;
mod outside;
pub mod peers;
pub mod persisted;
pub mod progress;
pub mod raw_text;
pub mod read_files;
pub mod repl;
pub(crate) mod script_bridge;
pub(crate) mod script_io;
pub mod script_tool;
#[cfg(test)]
pub(crate) mod scripted_tests;
pub mod seen_files;
pub mod skill;
mod skill_listing;
mod skill_script;
pub mod stopped;
pub mod structured;
pub mod subagent;
pub mod subagent_config;
pub mod tool_search;
pub mod watch;
mod workspace;

use std::path::Path;

use serde_json::Value;

pub use domain::permission::{Declared, Effect, SpawnKind};
pub use llm_providers::web_search::WEB_SEARCH;
pub(crate) use outside::settled;
pub use workspace::{
    CodePointers, Launch, Launcher, Models, Network, OnThisMachine, Pointers, Policy, Starter, WebSearcher, Workspace,
    env_secrets,
};

use crate::model::ToolSpec;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Outcome {
    pub content: String,
    pub is_error: bool,
    pub images: Vec<domain::Image>,
    #[serde(default)]
    pub shown: Option<String>,
    #[serde(default)]
    pub edited: Option<Box<domain::changes::Edited>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub background: Option<Box<domain::events::BackgroundChange>>,
}

impl Outcome {
    pub fn ok(content: impl Into<String>) -> Self {
        Self::new(content.into(), false, Vec::new())
    }

    fn new(content: String, is_error: bool, images: Vec<domain::Image>) -> Self {
        Self { content, is_error, images, shown: None, edited: None, background: None }
    }

    pub fn in_the_background(mut self, change: domain::events::BackgroundChange) -> Self {
        self.background = Some(Box::new(change));
        self
    }

    pub fn showing(mut self, shown: impl Into<String>) -> Self {
        self.shown = Some(shown.into());
        self
    }

    pub fn editing(mut self, edited: domain::changes::Edited) -> Self {
        self.edited = Some(Box::new(edited));
        self
    }

    pub fn image(image: domain::Image) -> Self {
        Self::new(format!("Image ({})", image.media_type), false, vec![image])
    }

    pub fn error(content: impl Into<String>) -> Self {
        Self::new(content.into(), true, Vec::new())
    }
}

pub struct Consult<'a> {
    pub api: &'a dyn crate::model::ModelApi,
    pub request: &'a crate::model::TurnRequest,
    pub conversation: &'a [crate::transcript::Message],
    pub stop: &'a platform::Stop,
}

impl std::fmt::Debug for Consult<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Consult").field("model", &self.request.model).finish_non_exhaustive()
    }
}

pub trait Tool: Send + Sync {
    fn spec(&self) -> ToolSpec;

    fn effects(&self, input: &Value, workspace: &Workspace) -> Vec<Effect>;

    fn summary(&self, input: &Value) -> String;

    fn run(&self, input: &Value, workspace: &Workspace) -> Outcome;

    fn offered(&self, _workspace: &Workspace) -> bool {
        true
    }

    fn shown(&self, _input: &Value, _workspace: &Workspace) -> Option<String> {
        None
    }

    fn serial(&self) -> bool {
        false
    }

    fn deferrable(&self) -> bool {
        false
    }

    fn top_level_only(&self) -> bool {
        self.script().is_some_and(|script| script.top_level_only)
    }

    fn script(&self) -> Option<&script_tool::ScriptTool> {
        None
    }

    fn skills(&self) -> &[skill::FoundSkill] {
        &[]
    }

    fn kept(&self) -> Option<std::sync::Arc<dyn Tool>> {
        None
    }
}

pub fn standard() -> Vec<Box<dyn Tool>> {
    script_tool::builtin().into_iter().map(|tool| Box::new(tool) as Box<dyn Tool>).collect()
}

pub fn named(name: &str) -> Option<Box<dyn Tool>> {
    standard().into_iter().find(|tool| tool.spec().name == name)
}

pub fn declared_path(name: &str) -> String {
    let typed = format!("tools/{name}.ts");
    if content::builtin(&typed).is_some() {
        return typed;
    }
    let offered = conditional::path(name);
    if content::builtin(&offered).is_some() { offered } else { format!("tools/{name}.js") }
}

pub fn declared(name: &str, data: &Value) -> crate::model::ToolSpec {
    let path = declared_path(name);
    let data = content::with_brand(data);
    let spec = content::value(&path, "spec").unwrap_or_default();
    let schema = match spec.get("input") {
        Some(input) => input.clone(),
        None => content::call(&path, "spec.input", &data).unwrap_or_else(|| serde_json::json!({ "type": "object" })),
    };
    crate::model::ToolSpec {
        name: name.to_owned(),
        description: content::render_export(&path, "spec.description", &data).unwrap_or_default(),
        schema,
    }
}

pub fn changes_nothing(tool: &dyn Tool, input: &Value, workspace: &Workspace) -> bool {
    domain::permission::change_nothing(&tool.effects(input, workspace))
}

pub(crate) fn field<'a>(input: &'a Value, name: &str) -> Option<&'a str> {
    input.get(name).and_then(Value::as_str)
}

pub(crate) fn clip(text: &str, limit: usize) -> String {
    match text.char_indices().nth(limit) {
        None => text.to_owned(),
        Some((at, _)) => format!("{}\n… [{} more characters cut]", &text[..at], text[at..].chars().count()),
    }
}

pub(crate) fn shown(path: &Path, workspace: &Workspace) -> String {
    path.strip_prefix(&workspace.cwd).unwrap_or(path).display().to_string()
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    #[test]
    fn resolves_paths_and_remembers_reads() {
        let workspace = Workspace::new("/repo");
        assert_eq!(workspace.resolve("src/a.rs"), PathBuf::from("/repo/src/a.rs"));
        assert_eq!(workspace.resolve("/etc/hosts"), PathBuf::from("/etc/hosts"));
        assert!(!workspace.has_read(Path::new("/repo/a")));
        workspace.mark_read(Path::new("/repo/a"));
        assert!(workspace.has_read(Path::new("/repo/a")));
    }

    #[test]
    fn clips_long_text() {
        assert_eq!(clip("abc", 5), "abc");
        assert_eq!(clip("abcdef", 3), "abc\n… [3 more characters cut]");
    }

    #[test]
    fn every_tool_has_a_distinct_name_and_an_object_schema() {
        let tools = standard();
        let names: std::collections::HashSet<String> = tools.iter().map(|tool| tool.spec().name).collect();
        assert_eq!(names.len(), tools.len());
        for tool in tools {
            assert_eq!(tool.spec().schema["type"], "object", "{}", tool.spec().name);
        }
    }
}

#[cfg(test)]
mod declared_tests {
    use super::Tool;

    #[test]
    fn every_built_in_tool_is_declared_in_content_with_an_execute() {
        let mut tools: Vec<Box<dyn Tool>> = super::standard();
        tools.extend(super::skill::tool(Vec::new()));
        let schema = serde_json::json!({ "type": "string" });
        let mut workspace = super::Workspace::new(".");
        tools.push(super::structured::offer(schema.clone(), &mut workspace).expect("structured"));
        let report = super::conditional::tool(domain::observer::REPORT_TOOL, &serde_json::Value::Null);
        tools.push(Box::new(report.expect("the observer's tool ships")));
        for tool in &tools {
            let spec = tool.spec();
            let name = spec.name.as_str();
            assert!(content::builtin(&super::declared_path(name)).is_some(), "{name} has no content/tools file");
            assert!(!spec.description.trim().is_empty(), "{name} has no description");
            assert_eq!(spec.schema["type"], "object", "{name}");
            assert!(tool.script().is_some(), "{name} is a script");
            assert!(!spec.description.contains("{{"), "{name} keeps a placeholder");
        }
        let structured = tools.iter().find(|tool| tool.spec().name == "StructuredOutput").expect("structured");
        assert_eq!(structured.spec().schema["properties"]["result"], schema, "input is a function of the data");
    }

    #[test]
    fn every_shipped_tool_script_keeps_the_whole_contract() {
        let scripts: Vec<(&str, String)> = content::builtin_files("tools/")
            .into_iter()
            .filter(|(path, _)| path.ends_with(".ts") && !path.ends_with(".d.ts"))
            .map(|(path, source)| (path, String::from_utf8_lossy(&source).into_owned()))
            .collect();
        assert!(scripts.len() > 30, "{}", scripts.len());
        for (path, source) in &scripts {
            for export in ["execute", "effects", "summary"] {
                assert!(script::exports(path, source, export), "{path} does not export {export}");
            }
            assert!(!source.contains("ctx.builtin("), "{path} still calls into a Rust tool");
            assert!(!source.lines().any(|line| line.trim_start().starts_with("access:")), "{path} declares access");
        }
    }
}
