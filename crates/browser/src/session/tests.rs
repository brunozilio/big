use std::time::Duration;

use serde_json::json;

use super::*;

fn settings(folder: &std::path::Path) -> Settings {
    Settings {
        call: Duration::from_secs(5),
        pairing: Duration::from_secs(1),
        upload_limit: 1024 * 1024,
        files: folder.join("files"),
    }
}

fn browser(device: &str) -> BrowserInfo {
    BrowserInfo {
        device_id: device.into(),
        name: format!("Chrome {device}"),
        platform: "mac".into(),
        version: "1".into(),
    }
}

fn fake_extension(hub: &Hub, device: &str, answer: fn(&Value) -> Value) -> std::thread::JoinHandle<Vec<Value>> {
    let (outbox, mut inbox) = tokio::sync::mpsc::unbounded_channel::<String>();
    hub.link(browser(device), outbox);
    let answering = hub.clone();
    let device = device.to_owned();
    std::thread::spawn(move || {
        let mut seen = Vec::new();
        while let Some(text) = inbox.blocking_recv() {
            let message: Value = serde_json::from_str(&text).unwrap_or_default();
            if message["type"] != "call" {
                continue;
            }
            answering.answer(&device, message["id"].as_u64().unwrap_or_default(), Ok(answer(&message)));
            seen.push(message);
        }
        seen
    })
}

fn call(session: &Session, name: &str, arguments: Value) -> Value {
    let message = json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": { "name": name, "arguments": arguments } });
    session.handle(&message).unwrap_or_default()["result"].clone()
}

fn first_text(result: &Value) -> String {
    result["content"][0]["text"].as_str().unwrap_or_default().to_owned()
}

fn echo(message: &Value) -> Value {
    if message["input"]["fail"] == true {
        return json!({ "content": [{ "type": "text", "text": "boom" }], "isError": true });
    }
    json!({ "content": [{ "type": "text", "text": format!("ran {}", message["tool"].as_str().unwrap_or_default()) }], "isError": false })
}

#[test]
fn the_server_lists_the_browser_tools_under_its_own_name() {
    let folder = tempfile::tempdir().unwrap_or_else(|_| unreachable!());
    let session = Session::new(Hub::new(), "s", settings(folder.path()), vec![]);
    let listed = session.handle(&json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/list" })).unwrap_or_default();
    assert_eq!(listed["result"]["tools"].as_array().map(Vec::len), Some(23));
    let initialized =
        session.handle(&json!({ "jsonrpc": "2.0", "id": 2, "method": "initialize", "params": {} })).unwrap_or_default();
    assert_eq!(initialized["result"]["serverInfo"]["name"], spec::server_name());
    let instructions = initialized["result"]["instructions"].as_str().unwrap_or_default();
    assert!(instructions.contains("tabs_context_mcp first"), "{instructions}");
    assert_eq!(session.local().name, spec::server_name());
}

#[test]
fn without_a_browser_every_page_tool_says_how_to_connect_one() {
    let folder = tempfile::tempdir().unwrap_or_else(|_| unreachable!());
    let session = Session::new(Hub::new(), "s", settings(folder.path()), vec![]);
    let result = call(&session, "tabs_context_mcp", json!({}));
    assert_eq!(result["isError"], true);
    assert!(first_text(&result).contains("not connected"));
    assert_eq!(call(&session, "list_connected_browsers", json!({}))["isError"], true);
}

#[test]
fn calls_reach_the_extension_with_the_session_and_its_answer_comes_back() {
    let folder = tempfile::tempdir().unwrap_or_else(|_| unreachable!());
    let hub = Hub::new();
    let worker = fake_extension(&hub, "a", echo);
    let session = Session::new(hub.clone(), "conversation-1", settings(folder.path()), vec![]);
    let result = call(&session, "navigate", json!({ "url": "example.com" }));
    assert_eq!(first_text(&result), "ran navigate");
    hub.unlink(1);
    let seen = worker.join().unwrap_or_default();
    assert_eq!(seen[0]["session"], "conversation-1");
    assert_eq!(seen[0]["standalone"], true);
    assert_eq!(seen[0]["input"]["url"], "example.com");
}

#[test]
fn a_batch_runs_in_order_labels_each_result_and_stops_at_the_first_error() {
    let folder = tempfile::tempdir().unwrap_or_else(|_| unreachable!());
    let hub = Hub::new();
    let worker = fake_extension(&hub, "a", echo);
    let session = Session::new(hub.clone(), "s", settings(folder.path()), vec![]);
    let result = call(
        &session,
        "browser_batch",
        json!({ "actions": [
            { "name": "computer", "input": { "action": "left_click", "coordinate": [1, 2], "tabId": 7 } },
            { "name": "computer", "input": { "action": "type", "text": "x", "tabId": 7, "fail": true } },
            { "name": "navigate", "input": { "url": "a.com", "tabId": 7 } }
        ]}),
    );
    assert_eq!(result["isError"], true);
    let texts: Vec<&str> = result["content"]
        .as_array()
        .map(|parts| parts.iter().filter_map(|part| part["text"].as_str()).collect())
        .unwrap_or_default();
    assert_eq!(
        texts,
        [
            "[1/3] computer: ran computer",
            "[2/3] computer: boom",
            "Batch stopped at action 2 of 3; the remaining 1 action(s) did not run."
        ]
    );
    hub.unlink(1);
    let seen = worker.join().unwrap_or_default();
    assert_eq!(seen.len(), 2);
    assert_eq!(seen[0]["standalone"], false);
}

#[test]
fn several_browsers_need_a_selection_that_sticks_to_the_session() {
    let folder = tempfile::tempdir().unwrap_or_else(|_| unreachable!());
    let hub = Hub::new();
    let first = fake_extension(&hub, "a", echo);
    let second = fake_extension(&hub, "b", echo);
    let session = Session::new(hub.clone(), "s", settings(folder.path()), vec![]);
    let refused = call(&session, "tabs_context_mcp", json!({}));
    assert!(first_text(&refused).contains("Several browsers are connected"));
    let listed: Value =
        serde_json::from_str(&first_text(&call(&session, "list_connected_browsers", json!({})))).unwrap_or_default();
    assert_eq!(listed.as_array().map(Vec::len), Some(2));
    assert_eq!(listed[0]["inUse"], false);
    assert_eq!(call(&session, "select_browser", json!({ "deviceId": "zzz" }))["isError"], true);
    assert!(
        first_text(&call(&session, "select_browser", json!({ "deviceId": "b" })))
            .contains("Selected browser \"Chrome b\"")
    );
    assert_eq!(first_text(&call(&session, "tabs_context_mcp", json!({}))), "ran tabs_context_mcp");
    hub.unlink(1);
    hub.unlink(2);
    assert!(first.join().unwrap_or_default().is_empty());
    assert_eq!(second.join().unwrap_or_default().len(), 1);
}

#[test]
fn find_ranks_the_candidates_the_extension_collects() {
    let folder = tempfile::tempdir().unwrap_or_else(|_| unreachable!());
    let hub = Hub::new();
    let worker = fake_extension(&hub, "a", |_| {
        json!({ "candidates": [
            { "ref": "ref_1", "role": "link", "name": "Home", "interactive": true },
            { "ref": "ref_2", "role": "button", "name": "Log in", "interactive": true }
        ]})
    });
    let session = Session::new(hub.clone(), "s", settings(folder.path()), vec![]);
    let found = call(&session, "find", json!({ "query": "log in button", "tabId": 1 }));
    assert!(
        first_text(&found).starts_with("Found 1 matching element:\n\n- ref_2: button \"Log in\""),
        "{}",
        first_text(&found)
    );
    hub.unlink(1);
    worker.join().unwrap_or_default();
}

#[test]
fn uploads_carry_the_file_bytes_and_unknown_tools_are_protocol_errors() {
    let folder = tempfile::tempdir().unwrap_or_else(|_| unreachable!());
    let file = folder.path().join("report.pdf");
    std::fs::write(&file, b"%PDF").unwrap_or_default();
    let hub = Hub::new();
    let worker = fake_extension(&hub, "a", echo);
    let session = Session::new(hub.clone(), "s", settings(folder.path()), vec![folder.path().to_path_buf()]);
    let result =
        call(&session, "file_upload", json!({ "paths": [file.display().to_string()], "ref": "ref_3", "tabId": 1 }));
    assert_eq!(first_text(&result), "ran file_upload");
    let unknown = session.handle(
        &json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": { "name": "nope", "arguments": {} } }),
    );
    assert!(unknown.is_some_and(|reply| reply.get("error").is_some()));
    hub.unlink(1);
    let seen = worker.join().unwrap_or_default();
    assert_eq!(seen[0]["input"]["files"][0]["mimeType"], "application/pdf");
}

#[test]
fn find_asks_the_model_when_one_is_given() {
    let folder = tempfile::tempdir().unwrap_or_else(|_| unreachable!());
    let hub = Hub::new();
    let worker = fake_extension(&hub, "a", |_| {
        json!({ "candidates": [
            { "ref": "ref_1", "role": "link", "name": "Home", "interactive": true },
            { "ref": "ref_2", "role": "button", "name": "Entrar", "interactive": true }
        ]})
    });
    let ask: Ask = Arc::new(|_, _| Some("{\"refs\": [\"ref_2\"]}".to_owned()));
    let session = Session::new(hub.clone(), "s", settings(folder.path()), vec![]).asking(Some(ask));
    let found = call(&session, "find", json!({ "query": "login button", "tabId": 1 }));
    assert!(
        first_text(&found).starts_with("Found 1 matching element:\n\n- ref_2: button \"Entrar\""),
        "{}",
        first_text(&found)
    );
    hub.unlink(1);
    worker.join().unwrap_or_default();
}
