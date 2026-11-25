use std::sync::{Arc, Mutex};

use agent_engine::Engine;
use agent_engine::model::scripted::ScriptedModel;
use agent_engine::sessions::InMemorySessions;
use agent_engine::transcript::Block;
use domain::{EngineEvent, PromptRequest};
use serde_json::json;

use crate::{Source, finish, model, start};

fn engine(scripted: &Arc<ScriptedModel>) -> Engine {
    Engine::new(Arc::new(Source(Mutex::new(vec![Arc::clone(scripted)]))), Arc::new(InMemorySessions::default()))
}

#[test]
fn in_bypass_a_command_runs_and_the_turn_limit_and_appended_prompt_apply() {
    let dir = tempfile::tempdir().expect("dir");
    let scripted = Arc::new(ScriptedModel::new([
        ScriptedModel::reply(vec![Block::ToolUse {
            id: "b".into(),
            name: "Bash".into(),
            input: json!({"command": "touch made.txt"}),
        }]),
        ScriptedModel::reply(vec![Block::Text("never asked".into())]),
    ]));
    let mut request = PromptRequest::plain(dir.path().into(), "make it", model());
    request.policy.reviewer = domain::Reviewer::Nobody;
    request.session.append_system = Some("Answer in French.".into());
    request.policy.max_turns = Some(1);
    let events = finish(&start(&engine(&scripted), &request));

    assert!(dir.path().join("made.txt").exists(), "bypass runs the command without asking");
    assert!(scripted.asked()[0].system.contains("\n\nAnswer in French."), "appended to the system prompt");
    assert_eq!(scripted.asked().len(), 1, "one turn only");
    assert!(matches!(events.last(), Some(EngineEvent::Failed(_))), "{events:?}");
}

#[test]
fn in_ask_mode_a_command_asks_first() {
    let dir = tempfile::tempdir().expect("dir");
    let scripted = Arc::new(ScriptedModel::new([
        ScriptedModel::reply(vec![Block::ToolUse {
            id: "b".into(),
            name: "Bash".into(),
            input: json!({"command": "touch made.txt"}),
        }]),
        ScriptedModel::reply(vec![Block::Text("done".into())]),
    ]));
    let request = PromptRequest::plain(dir.path().into(), "make it", model());
    let (events, mut handle) = crate::start_with_handle(&engine(&scripted), &request);

    let asked = std::iter::from_fn(|| events.recv_timeout(std::time::Duration::from_secs(10)).ok())
        .any(|event| matches!(event, EngineEvent::Asked(_)));

    assert!(asked, "the command waits for the user");
    assert!(!dir.path().join("made.txt").exists());
    handle.cancel();
}
