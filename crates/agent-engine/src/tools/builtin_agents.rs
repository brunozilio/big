use super::subagent::FoundAgent;
use super::subagent_config::AgentConfig;
use crate::transcript::{Block, Message, Role};

pub const FORK: &str = "fork";

const WRITERS: [&str; 4] = ["Agent", "Edit", "Write", "StructuredOutput"];

pub fn fork_preamble() -> String {
    content::text("agents/fork.js").trim().to_owned()
}

pub fn general() -> String {
    content::text("prompts/agent-general.js")
}

pub fn is_fork(input: &serde_json::Value) -> bool {
    input["subagent_type"].as_str() == Some(FORK)
}

pub fn fork_history(parent: &[Message], directive: &str) -> Vec<Message> {
    let mut history = parent.to_vec();
    let mut blocks: Vec<Block> = parent
        .last()
        .filter(|message| message.role == Role::Assistant)
        .map(|message| {
            message
                .tool_uses()
                .map(|(id, _, _)| Block::ToolResult {
                    id: id.to_owned(),
                    content: "Fork started: it runs apart and reports back.".into(),
                    is_error: false,
                })
                .collect()
        })
        .unwrap_or_default();
    blocks.push(Block::Text(format!("{}\n{directive}", fork_preamble())));
    history.push(Message { role: Role::User, blocks });
    history
}

fn read_only() -> AgentConfig {
    AgentConfig { disallowed_tools: WRITERS.map(str::to_owned).to_vec(), omit_instructions: true, ..Default::default() }
}

fn scripted(path: &str) -> Option<FoundAgent> {
    let spec = content::value(path, "spec")?;
    let read_only = spec["readOnly"].as_bool() == Some(true);
    Some(FoundAgent {
        name: spec["name"].as_str().filter(|name| !name.is_empty())?.to_owned(),
        description: spec["description"].as_str().unwrap_or_default().to_owned(),
        instructions: content::text(path).trim().to_owned(),
        config: if read_only { self::read_only() } else { AgentConfig::default() },
    })
}

fn written(path: &str) -> Option<FoundAgent> {
    let text = content::text(path);
    let (front, body) = crate::prompt::markdown::frontmatter(&text)
        .inspect_err(|error| tracing::error!(%error, path, "a built-in agent has a broken header"))
        .ok()?;
    let config =
        AgentConfig::parse(&front).unwrap_or_else(|error| AgentConfig { error: Some(error), ..Default::default() });
    let stem = path.rsplit('/').next().unwrap_or(path).trim_end_matches(".md");
    Some(FoundAgent {
        name: front["name"].as_str().unwrap_or(stem).trim().to_owned(),
        description: front["description"].as_str().unwrap_or_default().trim().to_owned(),
        instructions: body.trim().to_owned(),
        config,
    })
}

pub fn all() -> Vec<FoundAgent> {
    content::builtin_files("agents/")
        .into_iter()
        .filter_map(|(path, _)| {
            if path.ends_with(".js") {
                scripted(path)
            } else if path.ends_with(".md") {
                written(path)
            } else {
                None
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shipped() -> Vec<FoundAgent> {
        content::builtin_files("agents/")
            .into_iter()
            .filter(|(path, _)| path.ends_with(".md"))
            .filter_map(|(path, _)| {
                let stem = path.trim_start_matches("agents/").trim_end_matches(".md");
                content::builtin_text(path).map(|text| crate::tools::subagent::parsed(stem, &text, None, true))
            })
            .collect()
    }
    use domain::PermissionMode;

    #[test]
    fn read_only_agents_cannot_write_and_none_names_another_product() {
        for agent in all() {
            assert!(!agent.instructions.contains("Claude"), "{}", agent.name);
            assert!(!agent.instructions.contains("{READ_ONLY}"), "{}", agent.name);
        }
        let named = |name: &str| all().into_iter().find(|agent| agent.name == name).expect("built in");
        let explore = &named("Explore");
        assert!(explore.config.allows("Read") && explore.config.allows("Bash"));
        assert!(!explore.config.allows("Edit") && !explore.config.allows("Write") && !explore.config.allows("Agent"));
        assert!(explore.config.omit_instructions);
        assert!(named(FORK).config.allows("Edit"), "a fork keeps the parent's tools");
    }

    #[test]
    fn the_delivery_agents_are_built_in_with_their_mode_skills_and_avatar() {
        let named = |name: &str| shipped().into_iter().find(|agent| agent.name == name).expect(name);
        for (name, label, shape, color, mode) in [
            ("planner", "Planejador", "triangle", "#e94b9b", PermissionMode::Plan),
            ("developer", "Desenvolvedor", "squircle", "#3b82f6", PermissionMode::Bypass),
            ("reviewer", "Revisor", "circle", "#4fb39c", PermissionMode::Plan),
            ("release", "Release", "cloud", "#f0a235", PermissionMode::Plan),
        ] {
            let agent = named(name);
            assert_eq!(agent.config.error, None, "{name}");
            assert_eq!(agent.config.label.as_deref(), Some(label));
            let avatar = agent.config.avatar.as_ref().expect("avatar");
            assert_eq!((avatar.shape.as_str(), avatar.color.as_str()), (shape, color));
            assert_eq!(agent.config.permission, Some(mode), "{name}");
            assert_eq!(agent.config.model, None, "{name} leaves the model on auto");
            assert!(!agent.description.is_empty() && agent.instructions.starts_with("Você é"), "{name}");
            assert!(agent.instructions.contains(brand::NAME) && !agent.instructions.contains("{{"), "{name}");
        }
        assert!(!named("planner").config.allows("Edit") && !named("reviewer").config.allows("Write"));
        assert!(named("developer").config.allows("Edit") && !named("release").config.allows("Edit"));
        assert!(named("reviewer").config.skills.contains(&"code-review".to_owned()));
    }

    #[test]
    fn a_fork_answers_the_parents_calls_and_then_reads_its_directive() {
        let call = |id: &str| Block::ToolUse {
            id: id.into(),
            name: "Agent".into(),
            input: serde_json::json!({"subagent_type": "fork", "prompt": "x"}),
        };
        let parent = vec![Message::user("Ship it"), Message::assistant(vec![call("a"), call("b")])];
        let history = fork_history(&parent, "Check the tests");
        assert_eq!(history.len(), 3);
        assert_eq!(&history[..2], &parent[..]);
        let answers: Vec<&str> = history[2]
            .blocks
            .iter()
            .filter_map(|block| match block {
                Block::ToolResult { id, .. } => Some(id.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(answers, ["a", "b"]);
        assert!(history[2].text().ends_with("Your directive:\nCheck the tests"));
        assert!(is_fork(&serde_json::json!({"subagent_type": "fork"})));
        assert!(!is_fork(&serde_json::json!({"subagent_type": "Explore"})));
    }

    #[test]
    fn every_built_in_agent_names_only_real_tools_and_can_delegate_and_reach_mcp() {
        let written: Vec<FoundAgent> = shipped().into_iter().filter(|agent| agent.config.tools.is_some()).collect();
        assert!(written.len() >= 4, "the workflow's agents declare their tools");
        for agent in &written {
            assert_eq!(agent.config.error, None, "{}", agent.name);
            let listed = agent.config.tools.iter().flatten().chain(&agent.config.disallowed_tools);
            for name in listed.filter(|name| !name.ends_with('*')) {
                assert!(domain::ToolName::of(name).is_builtin(), "{} lists {name}, which is no tool", agent.name);
            }
            for tool in ["Agent", "TaskOutput", "BashOutput", "Monitor", "ToolSearch", "mcp__any__tool"] {
                assert!(agent.config.allows(tool), "{} cannot use {tool}", agent.name);
            }
        }
    }
}
