use agent_engine::sessions::{Entry, Sessions};
use domain::trace::runs::SpanKind;
use domain::{EngineEvent, ToolCall};

use super::*;

#[test]
fn another_folder_finds_nothing_to_trace() {
    let dir = tempfile::tempdir().expect("dir");
    let store = FileSessions::new(dir.path().join("sessions"));
    let id = SessionId("s1".into());
    store.record(dir.path(), &id, &Entry::Prompted { at_ms: 1, prompt: "hi".into() }).expect("record");
    assert!(traced(&store, dir.path(), &id).is_ok());
    assert!(traced(&store, &dir.path().join("elsewhere"), &id).is_err());
}

#[test]
fn a_traced_session_keeps_every_text_whole() {
    let root = tempfile::tempdir().expect("dir");
    let store = FileSessions::new(root.path().join("sessions"));
    let id = SessionId("long".into());
    let request = root.path().to_path_buf();
    let long = "x".repeat(30_000);
    for entry in [
        Entry::Prompted { at_ms: 1, prompt: long.clone() },
        Entry::Event { at_ms: 2, event: EngineEvent::ToolStarted(ToolCall::new("r", "Read", "a.txt")) },
        Entry::Event {
            at_ms: 3,
            event: EngineEvent::ToolResult { id: "r".into(), output: long.clone(), failed: false },
        },
    ] {
        store.record(&request, &id, &entry).expect("record");
    }

    let traced = traced(&store, root.path(), &id).expect("runs");
    let tool = traced[0].spans.iter().find(|span| span.kind == SpanKind::Tool).expect("tool");
    assert_eq!(tool.output.len(), 30_000);
}
