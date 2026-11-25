use std::collections::{BTreeMap, VecDeque};
use std::sync::{Mutex, PoisonError};

use async_trait::async_trait;
use serde_json::{Value, json};

use crate::error::ActionError;
use crate::http::{Http, HttpRequest, HttpResponse};

#[derive(Debug, Default)]
pub struct ZephyrMcp {
    answers: Mutex<BTreeMap<String, VecDeque<Value>>>,
    sent: Mutex<Vec<HttpRequest>>,
}

fn reply(id: &Value, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

impl ZephyrMcp {
    pub fn new() -> ZephyrMcp {
        ZephyrMcp::default()
    }

    pub fn answer(self, tool: &str, answer: Value) -> ZephyrMcp {
        self.answers
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .entry(tool.to_owned())
            .or_default()
            .push_back(answer);
        self
    }

    pub fn calls(&self) -> Vec<(String, Value)> {
        self.sent
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .iter()
            .map(HttpRequest::body_json)
            .filter(|body| body["method"] == "tools/call")
            .map(|body| {
                (body["params"]["name"].as_str().unwrap_or_default().to_owned(), body["params"]["arguments"].clone())
            })
            .collect()
    }

    fn tool(&self, params: &Value) -> Value {
        let name = params["name"].as_str().unwrap_or_default();
        let next =
            self.answers.lock().unwrap_or_else(PoisonError::into_inner).get_mut(name).and_then(VecDeque::pop_front);
        match next {
            Some(found) => {
                json!({ "content": [{ "type": "text", "text": found.to_string() }], "structuredContent": found })
            }
            None => {
                json!({ "isError": true, "content": [{ "type": "text", "text": format!("no answer for {name}") }] })
            }
        }
    }
}

#[async_trait]
impl Http for ZephyrMcp {
    async fn send(&self, request: HttpRequest) -> Result<HttpResponse, ActionError> {
        self.sent.lock().unwrap_or_else(PoisonError::into_inner).push(request.clone());
        let body = request.body_json();
        let id = &body["id"];
        Ok(match body["method"].as_str() {
            Some("initialize") => {
                HttpResponse::new(200, reply(id, json!({})).to_string()).with_header("mcp-session-id", "z1")
            }
            Some("tools/call") => HttpResponse::new(200, reply(id, self.tool(&body["params"])).to_string()),
            _ => HttpResponse::new(202, ""),
        })
    }
}
