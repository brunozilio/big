use std::sync::atomic::{AtomicU64, Ordering};

use serde_json::{Value, json};

use crate::error::ActionError;
use crate::http::{Http, HttpRequest, HttpResponse};

pub const PROTOCOL: &str = "2025-06-18";
const SESSION: &str = "mcp-session-id";

#[derive(Debug)]
pub struct McpClient<'a> {
    http: &'a dyn Http,
    url: String,
    token: String,
    session: Option<String>,
    next: AtomicU64,
}

pub fn messages(response: &HttpResponse) -> Vec<Value> {
    let text = response.text();
    let streamed = response.header("content-type").is_some_and(|kind| kind.contains("text/event-stream"))
        || text.trim_start().starts_with("event:")
        || text.trim_start().starts_with("data:");
    if !streamed {
        return serde_json::from_str::<Value>(&text).map(|value| vec![value]).unwrap_or_default();
    }
    let mut found = Vec::new();
    for event in text.split("\n\n") {
        let data: Vec<&str> = event
            .lines()
            .filter_map(|line| line.strip_prefix("data:"))
            .map(|line| line.strip_prefix(' ').unwrap_or(line))
            .collect();
        if let Ok(value) = serde_json::from_str::<Value>(&data.join("\n")) {
            found.push(value);
        }
    }
    found
}

pub fn tool_value(result: &Value) -> Result<Value, ActionError> {
    let texts: Vec<&str> = result["content"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .filter_map(|part| part["text"].as_str())
        .collect();
    let text = texts.join("\n");
    if result["isError"] == json!(true) {
        return Err(tool_error(&text));
    }
    if let Some(structured) = result.get("structuredContent").filter(|value| !value.is_null()) {
        return Ok(structured.clone());
    }
    Ok(serde_json::from_str(&text).unwrap_or(Value::String(text)))
}

fn tool_error(text: &str) -> ActionError {
    let lower = text.to_ascii_lowercase();
    let message = format!("the tool failed: {}", text.chars().take(500).collect::<String>());
    if lower.contains("unauthorized") || lower.contains("401") {
        ActionError::auth(message)
    } else if lower.contains("rate limit") || lower.contains("429") {
        ActionError::rate_limited(crate::error::DEFAULT_RETRY_AFTER, message)
    } else if lower.contains("timed out") || lower.contains("503") || lower.contains("502") {
        ActionError::transient(message)
    } else {
        ActionError::permanent(message)
    }
}

impl<'a> McpClient<'a> {
    pub async fn connect(http: &'a dyn Http, url: &str, token: &str) -> Result<McpClient<'a>, ActionError> {
        let mut client =
            McpClient { http, url: url.to_owned(), token: token.to_owned(), session: None, next: AtomicU64::new(1) };
        let params = json!({
            "protocolVersion": PROTOCOL,
            "capabilities": {},
            "clientInfo": { "name": brand::COMMAND, "version": env!("CARGO_PKG_VERSION") },
        });
        let (response, _) = client.exchange("initialize", params).await?;
        client.session = response.header(SESSION).map(str::to_owned);
        client.notify("notifications/initialized").await?;
        Ok(client)
    }

    fn request(&self, body: &Value) -> Result<HttpRequest, ActionError> {
        let mut request = HttpRequest::post(&self.url)
            .bearer(&self.token)
            .header("accept", "application/json, text/event-stream")
            .header("mcp-protocol-version", PROTOCOL);
        if let Some(session) = &self.session {
            request = request.header(SESSION, session.clone());
        }
        request.json(body)
    }

    async fn notify(&self, method: &str) -> Result<(), ActionError> {
        let body = json!({ "jsonrpc": "2.0", "method": method });
        self.http.send(self.request(&body)?).await?.checked()?;
        Ok(())
    }

    async fn exchange(&self, method: &str, params: Value) -> Result<(HttpResponse, Value), ActionError> {
        let id = self.next.fetch_add(1, Ordering::Relaxed);
        let body = json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params });
        let response = self.http.send(self.request(&body)?).await?.checked()?;
        let answer = messages(&response)
            .into_iter()
            .find(|message| message["id"] == json!(id))
            .ok_or_else(|| ActionError::decode(format!("no answer to {method}")))?;
        if let Some(error) = answer.get("error") {
            let text = error["message"].as_str().unwrap_or_default();
            return Err(ActionError::permanent(format!("{method} was refused: {text}")));
        }
        Ok((response, answer["result"].clone()))
    }

    pub async fn tools(&self) -> Result<Vec<String>, ActionError> {
        let (_, result) = self.exchange("tools/list", json!({})).await?;
        let listed = result["tools"].as_array().map(Vec::as_slice).unwrap_or_default();
        Ok(listed.iter().filter_map(|tool| tool["name"].as_str()).map(str::to_owned).collect())
    }

    pub async fn call(&self, tool: &str, arguments: Value) -> Result<Value, ActionError> {
        let (_, result) = self.exchange("tools/call", json!({ "name": tool, "arguments": arguments })).await?;
        tool_value(&result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::ErrorClass;
    use crate::http::{FakeHttp, Method};

    #[tokio::test]
    async fn a_session_is_initialized_and_tools_are_called_with_it() {
        let initialized = HttpResponse::new(200, r#"{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":"x"}}"#)
            .with_header("mcp-session-id", "s1");
        let streamed = "event: message\ndata: {\"jsonrpc\":\"2.0\",\"id\":2,\"result\":{\"content\":[{\"type\":\"text\",\"text\":\"{\\\"ok\\\":1}\"}]}}\n\n";
        let http =
            FakeHttp::new().respond(Method::Post, "/mcp", initialized).once(Method::Post, "/mcp", 202, "").respond(
                Method::Post,
                "/mcp",
                HttpResponse::new(200, streamed).with_header("content-type", "text/event-stream"),
            );
        let client = McpClient::connect(&http, "https://h/mcp", "tok").await;
        let Ok(client) = client else { panic!("{client:?}") };
        assert_eq!(client.call("thing", json!({"a": 1})).await, Ok(json!({"ok": 1})));
        let sent = http.sent();
        assert_eq!(sent[0].body_json()["method"], "initialize");
        assert_eq!(sent[1].body_json()["method"], "notifications/initialized");
        assert_eq!(sent[2].body_json()["params"]["name"], "thing");
        assert_eq!(sent[2].header_value("mcp-session-id"), Some("s1"));
        assert_eq!(sent[2].header_value("authorization"), Some("Bearer tok"));
    }

    #[tokio::test]
    async fn a_rejected_token_is_auth_and_a_tool_error_is_classified() {
        let http = FakeHttp::new().once(Method::Post, "/mcp", 401, r#"{"error":"invalid_token"}"#);
        let refused = McpClient::connect(&http, "https://h/mcp", "tok").await;
        assert_eq!(refused.map(|_| ()).map_err(|e| e.class), Err(ErrorClass::Auth));
        let failed =
            tool_value(&json!({"isError": true, "content": [{"type": "text", "text": "Issue does not exist"}]}));
        assert_eq!(failed.map_err(|e| e.class), Err(ErrorClass::Permanent));
        assert_eq!(tool_value(&json!({"content": [{"type": "text", "text": "plain"}]})), Ok(json!("plain")));
    }
}
