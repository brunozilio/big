use std::sync::Arc;
use std::sync::atomic::Ordering;

use agent_engine::lsp::fake::ScriptedServer;
use agent_engine::model::scripted::ScriptedModel;
use agent_engine::tools::Tool;
use agent_engine::transcript::{Block, Message};
use domain::{AskKind, Decision, EngineEvent, PermissionMode};
use serde_json::json;

use crate::{Rig, answering, tool_use};

const PENDING: &str = "install the rust-analyzer language server: download https://example.invalid/ra.gz";

fn rig(mode: PermissionMode, missing: bool) -> (Rig, Arc<ScriptedServer>) {
    let mut rig = Rig::new(mode);
    std::fs::write(rig.dir.path().join("main.rs"), "fn main() {}\n").expect("source");
    let server = ScriptedServer::answering([Ok(json!(null))]);
    if missing {
        *server.missing.lock().expect("missing") = Some(PENDING.into());
    }
    rig.workspace.services.lsp = Some(server.lsp());
    (rig, server)
}

fn hover(rig: &Rig) -> (String, bool) {
    let model = ScriptedModel::new([
        ScriptedModel::reply(vec![tool_use(
            "l",
            "LSP",
            json!({ "operation": "hover", "file_path": "main.rs", "line": 1, "character": 4 }),
        )]),
        ScriptedModel::reply(vec![Block::Text("Done".into())]),
    ]);
    let mut history = vec![Message::user("hover")];
    let extra: Vec<Box<dyn Tool>> = Vec::new();
    rig.run_with(&model, &mut history, extra).expect("run");
    history
        .iter()
        .flat_map(|message| message.blocks.iter())
        .find_map(|block| match block {
            Block::ToolResult { content, is_error, .. } => Some((content.clone(), *is_error)),
            _ => None,
        })
        .expect("a result")
}

fn asked(rig: &Rig) -> Vec<String> {
    rig.events()
        .into_iter()
        .filter_map(|event| match event {
            EngineEvent::Asked(ask) => match ask.kind {
                AskKind::Permission { subject, .. } => Some(subject),
                _ => None,
            },
            _ => None,
        })
        .collect()
}

#[test]
fn installing_a_missing_language_server_is_asked_first_and_a_refusal_installs_nothing() {
    let (rig, server) = rig(PermissionMode::AcceptEdits, true);
    let answerer = answering(&rig, Decision::Deny);
    let (content, is_error) = hover(&rig);
    answerer.join().expect("answerer");
    let asked = asked(&rig);
    assert_eq!(asked.len(), 1, "{asked:?}");
    assert!(asked[0].contains(PENDING), "the question says what would be installed: {asked:?}");
    assert!(is_error, "{content}");
    assert_eq!(server.installs.load(Ordering::SeqCst), 0);
}

#[test]
fn an_install_the_user_allows_runs_and_the_answer_follows() {
    let (rig, server) = rig(PermissionMode::AcceptEdits, true);
    let answerer = answering(&rig, Decision::Allow);
    let (content, is_error) = hover(&rig);
    answerer.join().expect("answerer");
    assert!(!is_error, "{content}");
    assert!(content.starts_with("Installed the rust-analyzer language server"), "{content}");
    assert_eq!(server.installs.load(Ordering::SeqCst), 1);
}

#[test]
fn installing_runs_without_asking_when_bypassing_permissions() {
    let (rig, server) = rig(PermissionMode::Bypass, true);
    let (content, is_error) = hover(&rig);
    assert!(!is_error, "{content}");
    assert!(asked(&rig).is_empty());
    assert_eq!(server.installs.load(Ordering::SeqCst), 1);
}

#[test]
fn installing_while_planning_is_refused_without_asking() {
    let (rig, server) = rig(PermissionMode::Plan, true);
    let (content, is_error) = hover(&rig);
    assert!(is_error && content.contains("planning"), "{content}");
    assert!(asked(&rig).is_empty());
    assert_eq!(server.installs.load(Ordering::SeqCst), 0);
}

#[test]
fn an_install_nobody_can_be_asked_about_is_refused() {
    let (mut rig, server) = rig(PermissionMode::Ask, true);
    rig.control = rig.control.child_configured(None, true);
    let (content, is_error) = hover(&rig);
    assert!(is_error && content.contains("requires permission"), "{content}");
    assert!(asked(&rig).is_empty());
    assert_eq!(server.installs.load(Ordering::SeqCst), 0);
}

#[test]
fn a_server_already_there_is_used_without_asking() {
    let (rig, server) = rig(PermissionMode::AcceptEdits, false);
    let (content, is_error) = hover(&rig);
    assert!(!is_error, "{content}");
    assert!(asked(&rig).is_empty());
    assert_eq!(server.installs.load(Ordering::SeqCst), 0);
}
