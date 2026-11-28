use std::sync::Arc;

use serde_json::{Value, json};

pub trait McpEndpoint: std::any::Any + Send + Sync {
    fn handle(&self, message: &Value) -> Option<Value>;

    fn handle_stopped(&self, message: &Value, _stop: &std::sync::atomic::AtomicBool) -> Option<Value> {
        self.handle(message)
    }
}

pub const PROTOCOL_VERSION: &str = "2025-06-18";
pub const TRACEPARENT: &str = "traceparent";

fn meta_traceparent(message: &Value) -> Option<&str> {
    message["params"]["_meta"][TRACEPARENT].as_str()
}

pub fn traceparent(message: &Value) -> Option<&str> {
    let one = meta_traceparent;
    let given = message.as_array().map_or_else(|| one(message), |all| all.iter().find_map(one))?;
    let hex = |part: &str, length: usize| part.len() == length && part.bytes().all(|byte| byte.is_ascii_hexdigit());
    let parts: Vec<&str> = given.split('-').collect();
    let valid = matches!(parts.as_slice(), [version, trace, span, flags]
        if hex(version, 2) && hex(trace, 32) && hex(span, 16) && hex(flags, 2));
    valid.then_some(given)
}

pub fn browser_server() -> String {
    format!("{}_browser", brand::COMMAND.replace('-', "_"))
}

pub fn connectors_server() -> String {
    format!("{}_connectors", brand::COMMAND.replace('-', "_"))
}

pub const CONNECTOR_TOOL_LIMIT: usize = 64;

pub fn connector_tool(label: &str, action: &str) -> String {
    let name = format!("{}_{}", slug(label), action.replace('.', "_"));
    name.chars().take(CONNECTOR_TOOL_LIMIT).collect()
}

pub fn served_name(server: &str, tool: &str) -> String {
    format!("mcp__{server}__{tool}")
}

pub type RpcError = (i64, String);

pub fn respond(
    message: &Value,
    server: &str,
    tools: impl FnOnce() -> Vec<Value>,
    call: impl FnOnce(&str, &Value) -> Result<Value, RpcError>,
) -> Option<Value> {
    let id = message.get("id")?.clone();
    let method = message.get("method").and_then(Value::as_str).unwrap_or_default();
    let params = message.get("params").cloned().unwrap_or(Value::Null);
    let result = match method {
        "initialize" => Ok(json!({
            "protocolVersion": params.get("protocolVersion").and_then(Value::as_str).unwrap_or(PROTOCOL_VERSION),
            "capabilities": { "tools": {} },
            "serverInfo": { "name": server, "version": env!("CARGO_PKG_VERSION") },
        })),
        "ping" => Ok(json!({})),
        "tools/list" => Ok(json!({ "tools": tools() })),
        "tools/call" => {
            let name = params.get("name").and_then(Value::as_str).unwrap_or_default();
            let arguments = params.get("arguments").cloned().unwrap_or(Value::Null);
            call(name, &arguments)
        }
        _ => Err((-32601, format!("method not found: {method}"))),
    };
    Some(match result {
        Ok(result) => json!({ "jsonrpc": "2.0", "id": id, "result": result }),
        Err((code, message)) => json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } }),
    })
}

pub fn text_result(text: impl Into<String>, error: bool) -> Value {
    json!({ "content": [{ "type": "text", "text": text.into() }], "isError": error })
}

#[derive(Clone)]
pub struct LocalMcp {
    pub name: String,
    pub endpoint: Arc<dyn McpEndpoint>,
}

impl LocalMcp {
    pub fn new(name: impl Into<String>, endpoint: Arc<dyn McpEndpoint>) -> Self {
        Self { name: name.into(), endpoint }
    }
}

impl std::fmt::Debug for LocalMcp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LocalMcp").field("name", &self.name).finish_non_exhaustive()
    }
}

impl PartialEq for LocalMcp {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name && Arc::ptr_eq(&self.endpoint, &other.endpoint)
    }
}

impl Eq for LocalMcp {}

pub struct McpBundle {
    server: String,
    parts: Vec<LocalMcp>,
    instructions: String,
}

impl McpBundle {
    pub fn new(server: impl Into<String>, parts: Vec<LocalMcp>) -> Self {
        Self { server: server.into(), parts, instructions: String::new() }
    }

    pub fn instructed(mut self, instructions: impl Into<String>) -> Self {
        self.instructions = instructions.into();
        self
    }

    fn answer(&self, message: &Value) -> Option<Value> {
        if let [only] = self.parts.as_slice() {
            return only.endpoint.handle(message);
        }
        match message["method"].as_str().unwrap_or_default() {
            "tools/call" => self.call(message),
            _ => respond(
                message,
                &self.server,
                || self.tools().into_iter().map(|(_, tool)| tool).collect(),
                |name, _| Err((-32602, format!("unknown tool: {name}"))),
            ),
        }
    }

    fn tools_of(part: &LocalMcp) -> Vec<Value> {
        let asked = json!({ "jsonrpc": "2.0", "id": 0, "method": "tools/list" });
        let listed = part.endpoint.handle(&asked).unwrap_or(Value::Null);
        listed["result"]["tools"].as_array().cloned().unwrap_or_default()
    }

    fn tools(&self) -> Vec<(usize, Value)> {
        let mut seen = std::collections::BTreeSet::new();
        let mut all = Vec::new();
        for (index, part) in self.parts.iter().enumerate() {
            for tool in Self::tools_of(part) {
                let name = tool["name"].as_str().unwrap_or_default().to_owned();
                if seen.insert(name) {
                    all.push((index, tool));
                }
            }
        }
        all
    }

    fn call(&self, message: &Value) -> Option<Value> {
        let name = message["params"]["name"].as_str().unwrap_or_default();
        let owner = self.tools().into_iter().find(|(_, tool)| tool["name"] == name).map(|(index, _)| index);
        match owner.and_then(|index| self.parts.get(index)) {
            Some(part) => part.endpoint.handle(message),
            None => respond(message, &self.server, Vec::new, |name, _| Err((-32602, format!("unknown tool: {name}")))),
        }
    }
}

pub struct McpPicked {
    part: LocalMcp,
    tools: Vec<String>,
}

impl McpPicked {
    pub fn local(part: LocalMcp, tools: Vec<String>) -> LocalMcp {
        if tools.is_empty() {
            return part;
        }
        let name = part.name.clone();
        LocalMcp::new(name, Arc::new(Self { part, tools }))
    }

    fn offers(&self, name: &str) -> bool {
        self.tools.iter().any(|tool| tool == name)
    }
}

impl std::fmt::Debug for McpPicked {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("McpPicked").field("part", &self.part).field("tools", &self.tools).finish()
    }
}

impl McpEndpoint for McpPicked {
    fn handle(&self, message: &Value) -> Option<Value> {
        match message["method"].as_str().unwrap_or_default() {
            "tools/list" => {
                let mut reply = self.part.endpoint.handle(message)?;
                if let Some(tools) = reply["result"]["tools"].as_array_mut() {
                    tools.retain(|tool| self.offers(tool["name"].as_str().unwrap_or_default()));
                }
                Some(reply)
            }
            "tools/call" if !self.offers(message["params"]["name"].as_str().unwrap_or_default()) => {
                respond(message, &self.part.name, Vec::new, |name, _| Err((-32602, format!("unknown tool: {name}"))))
            }
            _ => self.part.endpoint.handle(message),
        }
    }
}

impl std::fmt::Debug for McpBundle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("McpBundle").field("server", &self.server).field("parts", &self.parts).finish_non_exhaustive()
    }
}

impl McpEndpoint for McpBundle {
    fn handle(&self, message: &Value) -> Option<Value> {
        if message.get("id").is_none() {
            for part in &self.parts {
                part.endpoint.handle(message);
            }
            return None;
        }
        let mut reply = self.answer(message)?;
        if message["method"] == "initialize" && !self.instructions.is_empty() && reply["result"].is_object() {
            reply["result"]["instructions"] = json!(self.instructions);
        }
        Some(reply)
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct RemoteMcp {
    pub name: String,
    pub url: String,
    pub token: Option<String>,
}

pub fn slug(label: &str) -> String {
    let mut slug = String::new();
    for c in label.trim().chars().flat_map(char::to_lowercase) {
        if c.is_ascii_alphanumeric() {
            slug.push(c);
        } else if !slug.is_empty() && !slug.ends_with('-') {
            slug.push('-');
        }
    }
    slug.trim_end_matches('-').to_owned()
}

impl RemoteMcp {
    pub fn named(label: &str, url: &str, token: &str) -> Option<Self> {
        let name = slug(label);
        let token = (!token.is_empty()).then(|| token.to_owned());
        (!name.is_empty()).then(|| Self { name, url: url.to_owned(), token })
    }
}

impl std::fmt::Debug for RemoteMcp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RemoteMcp")
            .field("name", &self.name)
            .field("url", &self.url)
            .field("token", &self.token.is_some())
            .finish()
    }
}

#[cfg(test)]
mod bundle_tests;

#[cfg(test)]
mod remote_tests {
    use super::*;

    #[test]
    fn a_connection_label_becomes_a_tool_safe_name_and_its_token_never_prints() {
        assert_eq!(slug("  Linear Prod "), "linear-prod");
        assert_eq!(slug("Ação__MCP!!"), "a-o-mcp");
        assert_eq!(RemoteMcp::named("???", "https://x", "t"), None);
        let remote = RemoteMcp::named("Linear", "https://x/mcp", "secret-token");
        assert_eq!(remote.as_ref().map(|r| r.token.as_deref()), Some(Some("secret-token")));
        assert!(!format!("{remote:?}").contains("secret"));
        assert_eq!(RemoteMcp::named("Linear", "https://x/mcp", "").and_then(|r| r.token), None);
    }
}
