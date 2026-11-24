use super::agent_file;
use super::subagent::FoundAgent;
use crate::transcript::{Block, Message, Role};

pub const FORK: &str = "fork";

pub fn fork_preamble() -> String {
    content::render("agents/fork.ts", &content::with_brand(&serde_json::Value::Null)).trim().to_owned()
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

fn loaded(path: &str) -> Option<FoundAgent> {
    let source = content::source(path)?;
    let text = agent_file::markdown(path, &source, &crate::hooks::runs_from(path))
        .inspect_err(|error| tracing::error!(%error, path, "a built-in agent does not load"))
        .ok()?;
    Some(super::subagent::parsed(agent_file::stem(path), &text, None, true))
}

pub fn all() -> Vec<FoundAgent> {
    content::builtin_files("agents/")
        .into_iter()
        .filter(|(path, _)| agent_file::EXTENSIONS.iter().any(|ext| path.ends_with(&format!(".{ext}"))))
        .filter_map(|(path, _)| loaded(path))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shipped() -> Vec<FoundAgent> {
        all()
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
    fn the_agents_a_step_reads_a_result_from_declare_its_shape_and_every_one_is_observed() {
        let named = |name: &str| shipped().into_iter().find(|agent| agent.name == name).expect(name);
        for name in ["tester", "reviewer", "release"] {
            let agent = named(name);
            assert_eq!(agent.config.error, None, "{name}");
            assert!(agent.config.output_schema().is_some_and(|schema| schema.is_object()), "{name}");
        }
        for name in ["planner", "developer", "tester", "verifier", "reviewer", "release"] {
            let agent = named(name);
            assert_eq!(agent.config.observers, vec!["observer".to_owned()], "{name}");
            assert!(agent.config.allows("SendMessage") && agent.config.allows("ListSessions"), "{name}");
        }
        assert_eq!(named("observer").config.error, None);
    }

    #[test]
    fn the_delivery_agents_are_built_in_with_their_mode_skills_and_avatar() {
        let named = |name: &str| shipped().into_iter().find(|agent| agent.name == name).expect(name);
        for (name, label, shape, color, mode) in [
            ("planner", "Planner", "triangle", "#e94b9b", PermissionMode::Plan),
            ("developer", "Developer", "squircle", "#3b82f6", PermissionMode::Bypass),
            ("reviewer", "Reviewer", "circle", "#4fb39c", PermissionMode::Plan),
            ("release", "Release", "cloud", "#f0a235", PermissionMode::Plan),
        ] {
            let agent = named(name);
            assert_eq!(agent.config.error, None, "{name}");
            assert_eq!(agent.config.label.as_deref(), Some(label));
            let avatar = agent.config.avatar.as_ref().expect("avatar");
            assert_eq!((avatar.shape.as_str(), avatar.color.as_str()), (shape, color));
            assert_eq!(agent.config.permission, Some(mode), "{name}");
            assert_eq!(agent.config.model, None, "{name} leaves the model on auto");
            assert!(!agent.description.is_empty() && agent.instructions.starts_with("You are"), "{name}");
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
        let written: Vec<FoundAgent> = shipped()
            .into_iter()
            .filter(|agent| agent.config.tools.as_ref().is_some_and(|tools| !tools.is_empty()))
            .collect();
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
