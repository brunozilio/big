use std::sync::{Arc, Mutex};

use agent_engine::Engine;
use agent_engine::model::scripted::ScriptedModel;
use agent_engine::sessions::InMemorySessions;
use agent_engine::transcript::Block;
use domain::{EngineEvent, PromptRequest};

use crate::{Source, finish, start};

pub(crate) const SERVER: &str = r#"while IFS= read -r line; do
case "$line" in
*'"method":"initialize"'*) printf '%s\n' '{"jsonrpc":"2.0","id":1,"result":{}}';;
*'"method":"tools/list"'*) printf '%s\n' '{"jsonrpc":"2.0","id":2,"result":{"tools":[{"name":"ping","inputSchema":{"type":"object"}}]}}';;
esac
done"#;

fn offered(repo: &std::path::Path) -> Vec<String> {
    offered_to(&crate::asked_in(repo, "hi"))
}

fn offered_to(request: &PromptRequest) -> Vec<String> {
    let model_api = Arc::new(ScriptedModel::new([ScriptedModel::reply(vec![Block::Text("Hi".into())])]));
    let engine =
        Engine::new(Arc::new(Source(Mutex::new(vec![model_api.clone()]))), Arc::new(InMemorySessions::default()));
    let events = finish(&start(&engine, request));
    assert!(matches!(events.last(), Some(EngineEvent::Completed { .. })), "{events:?}");
    agent_engine::tools::repl::within_reach(&model_api.asked()[0].tools)
}

#[test]
#[cfg(unix)]
fn a_project_server_reaches_a_run_once_approved() {
    let repo = tempfile::tempdir().expect("repo");
    std::fs::write(repo.path().join("server.sh"), SERVER).expect("server");
    let config = format!(
        r#"{{"mcpServers": {{"jira": {{"command": "/bin/sh", "args": ["{}"]}}}}}}"#,
        repo.path().join("server.sh").display()
    );
    std::fs::create_dir_all(repo.path().join(brand::DIR)).expect("dir");
    std::fs::write(repo.path().join(concat!(brand::dir!(), "/mcp.json")), config).expect("config");
    assert!(!offered(repo.path()).iter().any(|name| name.starts_with("mcp__jira__")), "not approved yet");

    std::fs::write(
        repo.path().join(concat!(brand::dir!(), "/settings.local.json")),
        r#"{"enabledMcpServers": ["jira"]}"#,
    )
    .expect("approve");
    assert!(offered(repo.path()).contains(&"mcp__jira__ping".to_owned()));
}
