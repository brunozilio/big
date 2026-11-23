use std::collections::VecDeque;
use std::sync::{Mutex, PoisonError};

use async_trait::async_trait;
use serde_json::{Value, json};

use crate::error::ActionError;
use crate::http::{Http, HttpRequest, HttpResponse};

pub const TOOLS: [&str; 13] = [
    "getAccessibleAtlassianResources",
    "atlassianUserInfo",
    "getJiraIssue",
    "listJiraIssueComments",
    "searchJiraIssuesUsingJql",
    "createJiraIssue",
    "editJiraIssue",
    "transitionJiraIssue",
    "addOrEditJiraIssueComment",
    "discover",
    "executeRead",
    "executeWrite",
    "executeDestructive",
];
const SITELESS: [&str; 3] = ["getAccessibleAtlassianResources", "atlassianUserInfo", "discover"];
const OPERATIONS: [&str; 2] = ["listJiraIssueTransitions", "listJiraProjects"];

#[derive(Debug)]
pub struct AtlassianV2 {
    answers: Mutex<VecDeque<Value>>,
    structured: bool,
    operations: Vec<String>,
    sent: Mutex<Vec<HttpRequest>>,
}

fn reply(id: &Value, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

fn failure(text: &str) -> Value {
    json!({ "isError": true, "content": [{ "type": "text", "text": text }] })
}

impl AtlassianV2 {
    pub fn new(answers: &[Value]) -> AtlassianV2 {
        AtlassianV2 {
            answers: Mutex::new(answers.iter().cloned().collect()),
            structured: false,
            operations: OPERATIONS.iter().map(|name| (*name).to_owned()).collect(),
            sent: Mutex::new(Vec::new()),
        }
    }

    pub fn structured(answers: &[Value]) -> AtlassianV2 {
        AtlassianV2 { structured: true, ..AtlassianV2::new(answers) }
    }

    pub fn knowing(self, operations: &[&str]) -> AtlassianV2 {
        AtlassianV2 { operations: operations.iter().map(|name| (*name).to_owned()).collect(), ..self }
    }

    pub fn sent(&self) -> Vec<HttpRequest> {
        self.sent.lock().unwrap_or_else(PoisonError::into_inner).clone()
    }

    pub fn calls(&self) -> Vec<(String, Value)> {
        self.sent()
            .iter()
            .map(HttpRequest::body_json)
            .filter(|body| body["method"] == "tools/call")
            .map(|body| {
                (body["params"]["name"].as_str().unwrap_or_default().to_owned(), body["params"]["arguments"].clone())
            })
            .collect()
    }

    fn answer(&self) -> Value {
        let next = self.answers.lock().unwrap_or_else(PoisonError::into_inner).pop_front();
        let Some(found) = next else { return failure("no scripted answer left") };
        if self.structured {
            json!({ "content": [{ "type": "text", "text": "ok" }], "structuredContent": found })
        } else {
            let text = found.as_str().map_or_else(|| found.to_string(), str::to_owned);
            json!({ "content": [{ "type": "text", "text": text }] })
        }
    }

    fn tool(&self, id: &Value, params: &Value) -> Value {
        let name = params["name"].as_str().unwrap_or_default();
        let arguments = &params["arguments"];
        if !TOOLS.contains(&name) {
            let error = json!({ "code": -32602, "message": format!("Unknown tool: {name}") });
            return json!({ "jsonrpc": "2.0", "id": id, "error": error });
        }
        if !SITELESS.contains(&name) && arguments["cloudId"].as_str().is_none_or(str::is_empty) {
            return reply(id, failure("cloudId is required"));
        }
        let operation = arguments["name"].as_str().unwrap_or_default();
        if name.starts_with("execute") && !self.operations.iter().any(|known| known == operation) {
            return reply(
                id,
                failure(&format!("Unknown operation \"{operation}\". Call discover to find operation names.")),
            );
        }
        reply(id, self.answer())
    }
}

#[async_trait]
impl Http for AtlassianV2 {
    async fn send(&self, request: HttpRequest) -> Result<HttpResponse, ActionError> {
        self.sent.lock().unwrap_or_else(PoisonError::into_inner).push(request.clone());
        let body = request.body_json();
        let id = &body["id"];
        Ok(match body["method"].as_str() {
            Some("initialize") => {
                HttpResponse::new(200, reply(id, json!({})).to_string()).with_header("mcp-session-id", "s1")
            }
            Some("tools/call") => HttpResponse::new(200, self.tool(id, &body["params"]).to_string()),
            _ => HttpResponse::new(202, ""),
        })
    }
}
