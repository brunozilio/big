use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Duration;

use serde_json::{Value, json};

use crate::http::{Reply, exchange, place};

const SESSION_HEADER: &str = "mcp-session-id";
const MARGIN_SECONDS: u64 = 60;

#[derive(Debug, clap::Args)]
pub struct Args {}

#[derive(Debug)]
struct Bridge {
    data: PathBuf,
    session: Option<String>,
    wait: Duration,
}

fn wait() -> Duration {
    let call = brand::browser::CALL_SECONDS.trim().parse().unwrap_or(150);
    Duration::from_secs(call + MARGIN_SECONDS)
}

fn not_running() -> String {
    format!("{} is not running on this computer; start it, then call the tool again.", brand::NAME)
}

fn target(data: &Path) -> Result<(String, String), String> {
    let address = crate::instance::address(data).ok_or_else(not_running)?;
    let token = std::fs::read_to_string(data.join(brand::browser::MCP_TOKEN_FILE.trim()))
        .map(|text| text.trim().to_owned())
        .map_err(|_| not_running())?;
    let url = format!("{}{}", address.trim_end_matches('/'), brand::browser::MCP_ROUTE.trim());
    Ok((url, token))
}

fn failed(message: &Value, error: &str) -> Option<Value> {
    let id = message.get("id")?;
    Some(json!({ "jsonrpc": "2.0", "id": id, "error": { "code": -32000, "message": error } }))
}

fn reply_of(message: &Value, reply: &Reply) -> Option<Value> {
    if reply.code == 202 || (reply.code == 200 && reply.body.trim().is_empty()) {
        return None;
    }
    let parsed: Option<Value> = serde_json::from_str(reply.body.trim()).ok();
    match parsed {
        Some(value) if reply.code == 200 => Some(value),
        Some(value) if value["error"]["message"].is_string() => {
            failed(message, value["error"]["message"].as_str().unwrap_or_default())
        }
        _ => failed(message, &format!("{}: {}", reply.status, reply.body.trim())),
    }
}

impl Bridge {
    fn send(&mut self, message: &Value) -> Result<Reply, String> {
        let (url, token) = target(&self.data)?;
        let authorization = format!("Bearer {token}");
        let mut headers = vec![("Authorization", authorization.as_str()), ("Content-Type", "application/json")];
        let session = self.session.clone().unwrap_or_default();
        if !session.is_empty() {
            headers.push(("Mcp-Session-Id", session.as_str()));
        }
        let reply = exchange(&place(&url)?, "POST", &headers, &message.to_string(), self.wait)?;
        if let Some(id) = reply.header(SESSION_HEADER) {
            self.session = Some(id.to_owned());
        }
        Ok(reply)
    }

    fn relay(&mut self, line: &str) -> Option<Value> {
        let message: Value = match serde_json::from_str(line) {
            Ok(message) => message,
            Err(error) => {
                return Some(
                    json!({ "jsonrpc": "2.0", "id": null, "error": { "code": -32700, "message": error.to_string() } }),
                );
            }
        };
        match self.send(&message) {
            Ok(reply) => reply_of(&message, &reply),
            Err(error) => failed(&message, &error),
        }
    }
}

pub fn run(_args: Args, data: PathBuf) -> Result<ExitCode, String> {
    let mut bridge = Bridge { data, session: None, wait: wait() };
    let mut out = std::io::stdout().lock();
    for line in std::io::stdin().lock().lines() {
        let line = line.map_err(|error| error.to_string())?;
        if line.trim().is_empty() {
            continue;
        }
        if let Some(reply) = bridge.relay(&line) {
            writeln!(out, "{reply}").map_err(|error| error.to_string())?;
            out.flush().map_err(|error| error.to_string())?;
        }
    }
    Ok(ExitCode::SUCCESS)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reply(code: u16, body: &str) -> Reply {
        Reply { status: format!("HTTP/1.1 {code}"), code, headers: Vec::new(), body: body.to_owned() }
    }

    #[test]
    fn notifications_are_not_answered_and_replies_pass_through() {
        let asked = json!({ "jsonrpc": "2.0", "id": 7, "method": "tools/list" });
        assert!(reply_of(&json!({ "method": "notifications/initialized" }), &reply(202, "")).is_none());
        let passed = reply_of(&asked, &reply(200, r#"{"jsonrpc":"2.0","id":7,"result":{}}"#)).expect("reply");
        assert_eq!(passed["id"], 7);
    }

    #[test]
    fn a_refusal_becomes_an_error_for_the_same_request() {
        let asked = json!({ "jsonrpc": "2.0", "id": "a", "method": "tools/list" });
        let refused = r#"{"jsonrpc":"2.0","id":null,"error":{"code":-32000,"message":"turned off"}}"#;
        let answer = reply_of(&asked, &reply(503, refused)).expect("reply");
        assert_eq!((answer["id"].clone(), answer["error"]["message"].clone()), (json!("a"), json!("turned off")));
    }

    #[test]
    fn without_the_app_running_each_request_is_told_so() {
        let data = tempfile::tempdir().expect("data");
        let mut bridge = Bridge { data: data.path().to_path_buf(), session: None, wait: wait() };
        let answer = bridge.relay(r#"{"jsonrpc":"2.0","id":1,"method":"initialize"}"#).expect("reply");
        assert_eq!(answer["id"], 1);
        assert!(answer["error"]["message"].as_str().unwrap_or_default().contains("not running"));
        assert!(bridge.relay(r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#).is_none());
        assert_eq!(bridge.relay("not json").expect("parse error")["error"]["code"], -32700);
    }
}
