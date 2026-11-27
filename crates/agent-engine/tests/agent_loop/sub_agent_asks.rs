use std::sync::Arc;

use agent_engine::agent::Agent;
use agent_engine::model::TurnRequest;
use agent_engine::model::scripted::ScriptedModel;
use agent_engine::tools;
use agent_engine::transcript::{Block, Message};
use domain::{Decision, EngineEvent};
use serde_json::json;

use crate::{Rig, request, tool_use};

#[test]
fn an_ask_from_a_sub_agent_is_told_first_as_that_agent_s() {
    let rig = Rig::new(domain::Reviewer::Human);
    let model = ScriptedModel::new([
        ScriptedModel::reply(vec![tool_use(
            "a1",
            "Agent",
            json!({"description": "Write it", "prompt": "Write out.txt"}),
        )]),
        ScriptedModel::reply(vec![tool_use("b1", "Bash", json!({"command": "echo hi > out.txt"}))]),
        ScriptedModel::reply(vec![Block::Text("It was not allowed.".into())]),
        ScriptedModel::reply(vec![Block::Text("Nothing written.".into())]),
    ]);
    let tools = tools::standard();
    let events = Arc::clone(&rig.events);
    let emit = move |event: EngineEvent| events.lock().expect("events").push(event);
    let answerer = crate::answering(&rig, Decision::Deny);
    let agent = Agent {
        api: &model,
        tools: &tools,
        workspace: &rig.workspace,
        control: &rig.control,
        emit: &emit,
        request: TurnRequest { tools: Agent::specs(&tools), ..request() },
        agents: &[],
        depth: 0,
        turn_limit: agent_engine::agent::MAX_TURNS,
    };
    agent.run(&mut vec![Message::user("write it")]).expect("run");
    answerer.join().expect("answerer");

    let events = rig.events();
    let asked = events.iter().position(|event| matches!(event, EngineEvent::Asked(_))).expect("an ask");
    let EngineEvent::Asked(ask) = &events[asked] else { unreachable!() };
    assert!(
        matches!(&events[asked - 1], EngineEvent::AgentEvent { index: 0, event } if **event == EngineEvent::Asked(ask.clone())),
        "{:?}",
        events[asked - 1]
    );
}
