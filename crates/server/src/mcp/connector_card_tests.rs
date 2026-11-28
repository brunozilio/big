use serde_json::json;
use store::ConnectionRecord;

use super::{definition, server};

fn record() -> ConnectionRecord {
    ConnectionRecord {
        id: "c1".into(),
        provider: "fixture".into(),
        label: "Fixture".into(),
        account: None,
        capabilities: vec!["notes".into()],
        settings: json!({}),
    }
}

#[test]
fn the_approval_card_of_a_sending_action_shows_its_effects_and_recipients() {
    use connectors::Provider;
    let folder = tempfile::tempdir().expect("folder");
    let dir = folder.path().join("connectors/fixture");
    std::fs::create_dir_all(&dir).expect("dir");
    std::fs::write(dir.join("index.ts"), FIXTURE).expect("fixture");
    let fixture = connectors::scripted::ScriptedProvider::load_in(Some(folder.path()), "connectors/fixture/index.ts")
        .expect("fixture");
    let share = fixture.descriptor().action("notes.share").cloned().expect("notes.share");
    let tool = definition("fixture_notes_share", "Fixture", &record(), &share);
    assert_eq!(tool["annotations"]["openWorldHint"], true);
    let declared: domain::operation::Declaration =
        serde_json::from_value(tool["_meta"][domain::operation::META].clone()).expect("declared");
    let input = json!({ "to": ["ana@example.com", "rui@example.com"], "body": "…" });
    let operation = declared.operation(&server(), &input).expect("sending acts");
    let disclosed = domain::operation::Disclosed {
        strict: true,
        revoked_grant: true,
        ..domain::operation::Disclosed::of([&operation])
    };
    let kind = domain::AskKind::Permission {
        tool: "fixture_notes_share".into(),
        subject: "…".into(),
        rememberable: false,
        disclosed,
    };
    let card = crate::session_view::ask_event(&domain::AgentAsk { id: domain::AskId("a".into()), kind }, 0);
    assert_eq!((&card["strict"], &card["revokedGrant"]), (&json!(true), &json!(true)));
    assert_eq!(card["effects"], json!(["send-external", "pii"]));
    assert_eq!(card["recipients"], json!(["ana@example.com", "rui@example.com"]));
    assert_eq!(card["risk"], "high");
}

const FIXTURE: &str = r#"import { object, write } from "sdk/connectors/contracts.ts";
export function provider() {
  const share = { effects: ["send-external", "pii"], risk: "high", reversible: false, untrusted: false, sensitive: true, recipients: ["to"] };
  return { schema_version: 2, name: "Fixture", capabilities: ["notes"], auth: { kind: "token", fields: [] },
    actions: [write("notes.share", object([["to", "array"], ["body", "string"]]), object([]), "native", share)] };
}
export function call() { return null; }
"#;
