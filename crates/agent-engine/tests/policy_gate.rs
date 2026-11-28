#![allow(clippy::expect_used, reason = "a failed setup step stops the test")]

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use agent_engine::agent::Agent;
use agent_engine::control::Control;
use agent_engine::model::scripted::ScriptedModel;
use agent_engine::model::{ToolSpec, TurnRequest};
use agent_engine::tools::{Effect, Outcome, Tool, Workspace};
use agent_engine::transcript::{Block, Message};
use domain::operation::{Kind, Profile, Reversible, Risk};
use domain::permission::{Allowed, Behavior, Grant, Rule, allowed};
use domain::{Effort, EngineEvent};
use serde_json::{Value, json};

fn served() -> String {
    domain::mcp::served_name(&domain::mcp::connectors_server(), &domain::mcp::connector_tool("Fake", "code.merge"))
}

struct Merge {
    ran: Arc<AtomicUsize>,
}

impl Tool for Merge {
    fn spec(&self) -> ToolSpec {
        ToolSpec { name: served(), description: "Merge.".into(), schema: json!({ "type": "object" }) }
    }

    fn effects(&self, _input: &Value, _workspace: &Workspace) -> Vec<Effect> {
        let profile = Profile::new(vec![Kind::Write], Risk::Medium, Reversible::No, false, false);
        vec![Effect::Operation(profile.operation(&domain::mcp::connectors_server()))]
    }

    fn summary(&self, _input: &Value) -> String {
        "7".into()
    }

    fn run(&self, _input: &Value, _workspace: &Workspace) -> Outcome {
        self.ran.fetch_add(1, Ordering::SeqCst);
        Outcome::ok("merged")
    }
}

fn chatted(rule: Behavior) -> (String, usize) {
    allowed::install(Allowed::of([Rule::new(Grant::Tool(served()), rule)]), Arc::new(|_: &Allowed| {}));
    let dir = tempfile::tempdir().expect("dir");
    let workspace = Workspace::new(dir.path());
    let control = Control::new(domain::PermissionMode::Auto, Arc::default()).reviewed_by(domain::Reviewer::Nobody);
    let ran = Arc::new(AtomicUsize::new(0));
    let tools: Vec<Box<dyn Tool>> = vec![Box::new(Merge { ran: Arc::clone(&ran) })];
    let events: Arc<Mutex<Vec<EngineEvent>>> = Arc::default();
    let seen = Arc::clone(&events);
    let emit = move |event: EngineEvent| seen.lock().expect("events").push(event);
    let call = Block::ToolUse { id: "m".into(), name: served(), input: json!({ "pr": 7 }) };
    let model =
        ScriptedModel::new([ScriptedModel::reply(vec![call]), ScriptedModel::reply(vec![Block::Text("Ok.".into())])]);
    let agent = Agent {
        api: &model,
        tools: &tools,
        workspace: &workspace,
        control: &control,
        emit: &emit,
        request: TurnRequest {
            model: "m".into(),
            system: "s".into(),
            messages: Vec::new(),
            tools: Agent::specs(&tools),
            effort: Effort::Medium,
            max_tokens: 1000,
            intent: Default::default(),
        },
        agents: &[],
        depth: 0,
        turn_limit: agent_engine::agent::MAX_TURNS,
    };
    let mut history = vec![Message::user("merge")];
    agent.run(&mut history).expect("run");
    let told = history
        .iter()
        .flat_map(|message| message.blocks.iter())
        .find_map(|block| match block {
            Block::ToolResult { id, content, .. } if id == "m" => Some(content.clone()),
            _ => None,
        })
        .expect("a result");
    (told, ran.load(Ordering::SeqCst))
}

#[test]
fn the_deny_rule_that_stops_a_workflow_call_stops_the_same_tool_in_the_chat_and_an_allow_rule_still_runs_it() {
    let (told, ran) = chatted(Behavior::Deny);
    assert_eq!(ran, 0, "{told}");
    assert!(told.contains("denied by the permission list"), "{told}");
    let (told, ran) = chatted(Behavior::Allow);
    assert_eq!((told.as_str(), ran), ("merged", 1));
}
