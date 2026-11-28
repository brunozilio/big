use agent_engine::model::ToolSpec;
use agent_engine::model::scripted::ScriptedModel;
use agent_engine::tools::{Effect, Outcome, Tool, Workspace};
use agent_engine::transcript::{Block, Message};
use domain::EngineEvent;
use domain::label::{Held, Levels, SessionReads};
use domain::operation::{Kind, Profile, Reversible, Risk};
use serde_json::{Value, json};

use crate::{Rig, tool_use};

struct Shaped {
    name: &'static str,
    profile: Profile,
    says: &'static str,
}

impl Tool for Shaped {
    fn spec(&self) -> ToolSpec {
        ToolSpec { name: self.name.into(), description: String::new(), schema: json!({ "type": "object" }) }
    }

    fn effects(&self, _input: &Value, _workspace: &Workspace) -> Vec<Effect> {
        Vec::new()
    }

    fn summary(&self, _input: &Value) -> String {
        self.name.into()
    }

    fn run(&self, _input: &Value, _workspace: &Workspace) -> Outcome {
        Outcome::ok(self.says)
    }

    fn profile(&self) -> Option<Profile> {
        Some(self.profile.clone())
    }
}

fn shaped(name: &'static str, effects: Vec<Kind>, untrusted: bool, sensitive: bool) -> Box<dyn Tool> {
    let profile = Profile::new(effects, Risk::Low, Reversible::Yes, untrusted, sensitive);
    Box::new(Shaped { name, profile, says: "done" })
}

#[test]
fn with_the_shipped_labels_a_chat_keeps_the_three_properties_and_asks_nothing_new() {
    let mut rig = Rig::new(domain::Reviewer::Human);
    let reads = SessionReads::default();
    rig.workspace.credentials.exposed = Some(reads.clone());
    let model = ScriptedModel::new([
        ScriptedModel::reply(vec![tool_use("t1", "issue", json!({}))]),
        ScriptedModel::reply(vec![tool_use("t2", "vault", json!({}))]),
        ScriptedModel::reply(vec![tool_use("t3", "comment", json!({}))]),
        ScriptedModel::reply(vec![Block::Text("Ok.".into())]),
    ]);
    let extra = vec![
        shaped("issue", vec![Kind::Read], true, false),
        shaped("vault", vec![Kind::Read], false, true),
        shaped("comment", vec![Kind::Write, Kind::SendExternal], false, false),
    ];
    rig.run_with(&model, &mut vec![Message::user("go")], extra).expect("run");
    let events = rig.events();
    assert!(!events.iter().any(|event| matches!(event, EngineEvent::Asked(_))), "the shipped rule of two only logs");
    assert!(!events.iter().any(|event| matches!(event, EngineEvent::ToolResult { failed: true, .. })));
    let held = Held::of(&Levels::default(), &reads.held());
    assert_eq!(held.untrusted.as_deref(), Some("issue"));
    assert_eq!(held.sensitive.as_deref(), Some("vault"));
    assert_eq!(held.acted.as_deref(), Some("comment"));
}
