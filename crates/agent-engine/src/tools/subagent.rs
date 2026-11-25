use std::path::{Path, PathBuf};

use super::subagent_config::AgentConfig;
use domain::Folder;
use serde_json::{Value, json};

use super::Tool;
use super::names::AGENT;
use super::script_tool::ScriptTool;

pub const GENERAL: &str = "general-purpose";

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct FoundAgent {
    pub name: String,
    pub description: String,
    pub instructions: String,
    #[serde(default)]
    pub config: AgentConfig,
}

pub fn discover(folder: Folder<'_>, home: Option<&Path>) -> Vec<FoundAgent> {
    let cwd = folder.cwd;
    let gate = crate::scope::Scope::within(folder, home);
    let project_folders = cwd.ancestors().count();
    let mut sources: Vec<(PathBuf, Option<String>)> = Vec::new();
    for directory in cwd.ancestors() {
        sources.push((directory.join(concat!(brand::dir!(), "/agents")), None));
    }
    if let Some(home) = home {
        sources.push((home.join(concat!(brand::dir!(), "/agents")), None));
    }
    for dir in folder.extensions.iter().flat_map(|extensions| &extensions.dirs) {
        sources.push((dir.join("agents"), None));
    }
    let mut found: Vec<FoundAgent> = Vec::new();
    let scanned = sources.into_iter().enumerate().flat_map(|(at, (dir, namespace))| {
        let privileged = |file: &Path| namespace.is_none() && (at >= project_folders || gate.honours(file));
        scan(&dir, namespace.as_deref(), &privileged)
    });
    for agent in scanned.chain(super::builtin_agents::all()) {
        if !found.iter().any(|known| known.name == agent.name) {
            found.push(agent);
        }
    }
    found
}

pub const PRIVILEGES: [&str; 5] = ["hooks", "intercept", "mcpServers", "permissionMode", "initialPrompt"];

fn scan(dir: &Path, namespace: Option<&str>, privileged: &dyn Fn(&Path) -> bool) -> Vec<FoundAgent> {
    let Ok(entries) = std::fs::read_dir(dir) else { return Vec::new() };
    let mut agents: Vec<FoundAgent> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.is_file()
                && path.extension().is_some_and(|ext| super::agent_file::EXTENSIONS.iter().any(|known| ext == *known))
        })
        .filter_map(|path| {
            let source = std::fs::read_to_string(&path).ok()?;
            let stem = path.file_stem()?.to_string_lossy().into_owned();
            let shown = path.to_string_lossy();
            Some(match super::agent_file::markdown(&shown, &source, &shown) {
                Ok(text) => parsed(&stem, &text, namespace, privileged(&path)),
                Err(error) => FoundAgent {
                    name: namespace.map_or_else(|| stem.clone(), |namespace| format!("{namespace}:{stem}")),
                    description: String::new(),
                    instructions: String::new(),
                    config: AgentConfig { error: Some(error), ..Default::default() },
                },
            })
        })
        .collect();
    agents.sort_by(|a, b| a.name.cmp(&b.name));
    agents
}

pub fn parsed(stem: &str, text: &str, namespace: Option<&str>, privileged: bool) -> FoundAgent {
    let (front, body, config) = match crate::prompt::markdown::frontmatter(text) {
        Ok((mut front, body)) => {
            if !privileged && let Some(map) = front.as_object_mut() {
                for key in PRIVILEGES {
                    map.remove(key);
                }
            }
            let config = AgentConfig::parse(&front)
                .unwrap_or_else(|error| AgentConfig { error: Some(error), ..Default::default() });
            (front, body, config)
        }
        Err(error) => (Value::Null, "", AgentConfig { error: Some(error), ..Default::default() }),
    };
    let bare = front["name"].as_str().unwrap_or(stem).trim().to_owned();
    let name = match namespace {
        Some(namespace) => format!("{namespace}:{bare}"),
        None => bare,
    };
    let description = front["description"].as_str().unwrap_or_default().trim().to_owned();
    FoundAgent { name, description, instructions: body.trim().to_owned(), config }
}

pub fn chosen<'a>(agents: &'a [FoundAgent], input: &Value) -> Result<Option<&'a FoundAgent>, String> {
    let chosen = match input["subagent_type"].as_str().filter(|kind| !kind.is_empty() && *kind != GENERAL) {
        None => Ok(None),
        Some(kind) => agents.iter().find(|agent| agent.name == kind).map(Some).ok_or_else(|| {
            format!("There is no agent named {kind}. Leave subagent_type out for a general-purpose agent.")
        }),
    }?;
    if let Some(agent) = chosen
        && let Some(error) = &agent.config.error
    {
        return Err(format!("Agent {}: {error}", agent.name));
    }
    Ok(chosen)
}

pub fn tool(agents: &[FoundAgent]) -> Option<ScriptTool> {
    let mut found = super::script_tool::builtin().into_iter().find(|tool| tool.spec.name == AGENT)?;
    let listed: Vec<Value> =
        agents.iter().map(|agent| json!({ "name": agent.name, "description": agent.description })).collect();
    found.spec = super::declared(AGENT, &json!({ "agents": listed }));
    Some(found)
}

pub fn offer(tools: &mut Vec<Box<dyn Tool>>, agents: &[FoundAgent]) {
    super::script_tool::join(tools, tool(agents).into_iter().collect());
}

pub fn system(parent: &str, agent: Option<&FoundAgent>) -> String {
    let agent = agent.map(|agent| serde_json::json!({ "name": agent.name, "instructions": agent.instructions }));
    let data = serde_json::json!({ "parent": parent, "agent": agent, "general": GENERAL });
    crate::prompt::render("prompts/sub-agent.js", &data)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_custom_agents_and_picks_the_one_asked_for() {
        let repo = tempfile::tempdir().expect("repo");
        let dir = repo.path().join(concat!(brand::dir!(), "/agents"));
        std::fs::create_dir_all(&dir).expect("dir");
        std::fs::write(
            dir.join("reviewer.md"),
            "---\nname: reviewer\ndescription: Reviews diffs.\n---\n\nBe strict.\n",
        )
        .expect("write");
        let folder = Folder { cwd: repo.path(), extensions: None };
        let agents = discover(folder, None);
        let offered = tool(&agents).expect("the Agent tool ships");
        assert!(offered.script().is_some(), "Agent is a script tool");
        let description = offered.spec().description;
        let project = description.find("- reviewer: Reviews diffs.\n- ");
        let shipped = description.find("- developer: ");
        assert!(
            project.zip(shipped).is_some_and(|(project, shipped)| project < shipped),
            "project agents first: {description}"
        );
        assert!(description.contains("- fork: "));
        let reviewer = chosen(&agents, &json!({ "subagent_type": "reviewer" })).expect("known").expect("custom");
        assert_eq!(reviewer.instructions, "Be strict.");
        assert_eq!(chosen(&agents, &json!({ "subagent_type": "general-purpose" })), Ok(None));
        assert!(chosen(&agents, &json!({ "subagent_type": "ghost" })).is_err());
        let prompt = system("Parent.", Some(reviewer));
        assert!(prompt.starts_with("Parent.\n\n# You are a sub-agent"));
        assert!(prompt.ends_with("# Your instructions (reviewer)\nBe strict."));
    }

    #[test]
    fn a_project_agent_written_in_typescript_is_found_and_a_broken_one_says_why() {
        let repo = tempfile::tempdir().expect("repo");
        let dir = repo.path().join(concat!(brand::dir!(), "/agents"));
        std::fs::create_dir_all(&dir).expect("dir");
        let source = "export const spec = { name: \"auditor\", description: \"Audits.\", tools: [\"Read\"] };\nexport default `Audit it.`;";
        std::fs::write(dir.join("auditor.ts"), source).expect("write");
        std::fs::write(dir.join("broken.ts"), "export const spec = {};").expect("write");
        let agents = discover(Folder { cwd: repo.path(), extensions: None }, None);
        let auditor = chosen(&agents, &json!({ "subagent_type": "auditor" })).expect("known").expect("custom");
        assert_eq!((auditor.description.as_str(), auditor.instructions.as_str()), ("Audits.", "Audit it."));
        assert!(auditor.config.allows("Read") && !auditor.config.allows("Edit"));
        assert!(chosen(&agents, &json!({ "subagent_type": "broken" })).is_err());
    }
}
