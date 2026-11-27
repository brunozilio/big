use std::sync::{Arc, Mutex};

use serde_json::{Value, json};

use super::{LocalMcp, McpBundle, McpEndpoint, respond, text_result};

struct Named {
    server: &'static str,
    tools: &'static [&'static str],
    heard: Mutex<Vec<String>>,
}

impl McpEndpoint for Named {
    fn handle(&self, message: &Value) -> Option<Value> {
        let method = message["method"].as_str().unwrap_or_default().to_owned();
        self.heard.lock().unwrap_or_else(std::sync::PoisonError::into_inner).push(method);
        respond(
            message,
            self.server,
            || self.tools.iter().map(|name| json!({ "name": name })).collect(),
            |name, _| Ok(text_result(format!("{}:{name}", self.server), false)),
        )
    }
}

fn part(server: &'static str, tools: &'static [&'static str]) -> (LocalMcp, Arc<Named>) {
    let named = Arc::new(Named { server, tools, heard: Mutex::default() });
    (LocalMcp::new(server, named.clone()), named)
}

fn ask(bundle: &McpBundle, method: &str, params: Value) -> Value {
    bundle.handle(&json!({ "jsonrpc": "2.0", "id": 9, "method": method, "params": params })).expect("answer")
}

#[test]
fn several_servers_answer_as_one_and_each_call_reaches_its_owner() {
    let (memory, _) = part("memory", &["memory_search", "shared"]);
    let (browser, heard) = part("browser", &["navigate", "shared"]);
    let bundle = McpBundle::new("all", vec![memory, browser]);
    assert_eq!(ask(&bundle, "initialize", json!({}))["result"]["serverInfo"]["name"], "all");
    let names: Vec<Value> = ask(&bundle, "tools/list", Value::Null)["result"]["tools"]
        .as_array()
        .map(|tools| tools.iter().map(|tool| tool["name"].clone()).collect())
        .unwrap_or_default();
    assert_eq!(names, vec![json!("memory_search"), json!("shared"), json!("navigate")]);
    let call = |name: &str| ask(&bundle, "tools/call", json!({ "name": name }))["result"]["content"][0]["text"].clone();
    assert_eq!(call("navigate"), "browser:navigate");
    assert_eq!(call("shared"), "memory:shared");
    assert_eq!(ask(&bundle, "tools/call", json!({ "name": "nope" }))["error"]["code"], -32602);
    assert!(bundle.handle(&json!({ "method": "notifications/initialized" })).is_none());
    assert!(heard.heard.lock().expect("heard").contains(&"notifications/initialized".to_owned()));
}

#[test]
fn a_single_server_keeps_its_own_name() {
    let (browser, _) = part("browser", &["navigate"]);
    let bundle = McpBundle::new("all", vec![browser]).instructed("browser: drive the browser.");
    let hello = ask(&bundle, "initialize", json!({}));
    assert_eq!(hello["result"]["serverInfo"]["name"], "browser");
    assert_eq!(hello["result"]["instructions"], "browser: drive the browser.");
    assert!(ask(&bundle, "tools/list", Value::Null)["result"].get("instructions").is_none());
}

#[test]
fn a_picked_server_lists_and_runs_only_the_tools_named() {
    let (memory, _) = part("memory", &["memory_search", "memory_forget"]);
    let picked = super::McpPicked::local(memory, vec!["memory_search".into()]);
    let bundle = McpBundle::new("all", vec![picked]);
    let tools = ask(&bundle, "tools/list", Value::Null)["result"]["tools"].clone();
    assert_eq!(tools, json!([{ "name": "memory_search" }]));
    assert_eq!(ask(&bundle, "tools/call", json!({ "name": "memory_forget" }))["error"]["code"], -32602);
    assert_eq!(
        ask(&bundle, "tools/call", json!({ "name": "memory_search" }))["result"]["content"][0]["text"],
        "memory:memory_search"
    );
}
