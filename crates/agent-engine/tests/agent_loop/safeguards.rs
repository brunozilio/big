use agent_engine::model::ToolSpec;
use agent_engine::model::scripted::ScriptedModel;
use agent_engine::tools::tool_search::defer;
use agent_engine::tools::{Effect, Outcome, Tool, Workspace};
use agent_engine::transcript::{Block, Message};
use domain::PermissionMode;
use serde_json::{Value, json};

use crate::{Rig, tool_use};

fn first_result(model: &ScriptedModel) -> String {
    let results = model.asked()[1].messages.last().cloned().expect("results");
    match &results.blocks[0] {
        Block::ToolResult { content, .. } => content.clone(),
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_long_output_is_saved_to_a_file_the_agent_may_read() {
    let rig = Rig::new(PermissionMode::Bypass);
    let model = ScriptedModel::new([
        ScriptedModel::reply(vec![tool_use("big", "Bash", json!({"command": "yes line | head -n 20000"}))]),
        ScriptedModel::reply(vec![Block::Text("Done.".into())]),
    ]);
    rig.run(&model, &mut vec![Message::user("print a lot")]).expect("run");
    let content = first_result(&model);
    assert!(content.starts_with("[Output cut: 99999 characters in 20000 lines"), "{content}");
    assert!(content.chars().count() < domain::output::DEFAULT_LIMIT + 400, "{}", content.len());
    let path = content
        .split("The full output is in ")
        .nth(1)
        .and_then(|rest| rest.split(": Read or Grep it for the rest.]").next())
        .expect("path");
    assert_eq!(std::fs::read_to_string(path).expect("saved").lines().count(), 20_000);
    assert!(rig.workspace.may_read(std::path::Path::new(path)));
}

#[test]
fn tools_waiting_behind_tool_search_are_named_in_a_reminder() {
    let mut rig = Rig::new(PermissionMode::Bypass);
    let specs = (0..25).map(|n| ToolSpec {
        name: format!("mcp__jira__tool{n}"),
        description: String::new(),
        schema: json!({}),
    });
    rig.workspace.services.deferred = defer(specs.collect()).1.map(std::sync::Arc::new);
    let model = ScriptedModel::new([ScriptedModel::reply(vec![Block::Text("Done.".into())])]);
    rig.run_with(&model, &mut vec![Message::user("hi")], Vec::new()).expect("run");
    let sent = &model.asked()[0].messages[0];
    let reminder =
        sent.blocks.iter().find_map(|b| if let Block::Reminder(text) = b { Some(text.as_str()) } else { None });
    assert!(reminder.is_some_and(|text| text.contains("mcp__jira__tool24")), "{sent:?}");
}

struct Waiting;

impl Tool for Waiting {
    fn spec(&self) -> ToolSpec {
        ToolSpec { name: "mcp__jira__tool3".into(), description: String::new(), schema: json!({"type": "object"}) }
    }

    fn effects(&self, _input: &Value, _workspace: &Workspace) -> Vec<Effect> {
        Vec::new()
    }

    fn summary(&self, _input: &Value) -> String {
        String::new()
    }

    fn run(&self, _input: &Value, _workspace: &Workspace) -> Outcome {
        Outcome::ok("three issues")
    }
}

fn calling_a_waiting_tool(load_on_call: bool) -> (ScriptedModel, String) {
    let mut rig = Rig::new(PermissionMode::Bypass);
    rig.workspace.policy.load_on_call = load_on_call;
    let specs = (0..25).map(|n| ToolSpec {
        name: format!("mcp__jira__tool{n}"),
        description: String::new(),
        schema: json!({"type": "object"}),
    });
    rig.workspace.services.deferred = defer(specs.collect()).1.map(std::sync::Arc::new);
    let model = ScriptedModel::new([
        ScriptedModel::reply(vec![tool_use("t", "mcp__jira__tool3", json!({}))]),
        ScriptedModel::reply(vec![Block::Text("Done.".into())]),
    ]);
    let tools: Vec<Box<dyn Tool>> = vec![Box::new(Waiting)];
    rig.run_hiding(&model, &mut vec![Message::user("list issues")], tools, &["mcp__jira__tool3"]).expect("run");
    let result = first_result(&model);
    (model, result)
}

#[test]
fn a_waiting_tool_called_by_name_is_loaded_and_runs() {
    let (model, result) = calling_a_waiting_tool(true);
    assert!(result.contains("three issues") && !result.contains("not available"), "{result}");
    assert!(model.asked()[1].tools.iter().any(|spec| spec.name == "mcp__jira__tool3"), "offered from then on");
}

#[test]
fn with_load_on_call_off_a_waiting_tool_must_go_through_tool_search() {
    let (_, result) = calling_a_waiting_tool(false);
    assert!(result.contains("is not available to this agent"), "{result}");
}
