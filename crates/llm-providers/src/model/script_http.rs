use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use serde_json::{Map, Value, json};

use super::framing::{Framed, Framing};
use super::http::{Http, Outgoing, Response, reason};
use super::{CancellationToken, ModelError};

pub const READ: usize = 5 << 20;
pub const READ_AT_MOST: usize = 32 << 20;
pub const OPEN_AT_MOST: usize = 8;
const ERROR_READ: usize = 64 << 10;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Asked {
    pub method: String,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: Option<String>,
}

impl Asked {
    pub fn read(arguments: &Value) -> Result<Self, String> {
        let url = arguments.get("url").and_then(Value::as_str).ok_or("url must be a string")?.to_owned();
        let body = match arguments.get("body") {
            None | Some(Value::Null) => None,
            Some(Value::String(text)) => Some(text.clone()),
            Some(value) => Some(value.to_string()),
        };
        let method = arguments
            .get("method")
            .and_then(Value::as_str)
            .map_or_else(|| if body.is_some() { "POST" } else { "GET" }.to_owned(), str::to_ascii_uppercase);
        Ok(Self { method, url, headers: pairs(arguments.get("headers")), body })
    }

    fn outgoing(&self, streaming: bool) -> Outgoing<'_> {
        Outgoing { method: &self.method, url: &self.url, headers: &self.headers, body: self.body.as_deref(), streaming }
    }
}

pub fn pairs(value: Option<&Value>) -> Vec<(String, String)> {
    match value {
        Some(Value::Object(fields)) => fields
            .iter()
            .filter_map(|(name, value)| Some((name.clone(), value.as_str()?.to_owned())))
            .collect(),
        Some(Value::Array(pairs)) => pairs
            .iter()
            .filter_map(|pair| Some((pair.get(0)?.as_str()?.to_owned(), pair.get(1)?.as_str()?.to_owned())))
            .collect(),
        _ => Vec::new(),
    }
}

fn headers_of(response: &Response) -> Value {
    let mut headers = Map::new();
    for (name, value) in &response.headers {
        let name = name.to_ascii_lowercase();
        let joined = match headers.get(&name).and_then(Value::as_str) {
            Some(earlier) => format!("{earlier}, {value}"),
            None => value.clone(),
        };
        headers.insert(name, Value::String(joined));
    }
    Value::Object(headers)
}

fn most_of(arguments: &Value) -> usize {
    arguments
        .get("most")
        .and_then(Value::as_u64)
        .and_then(|most| usize::try_from(most).ok())
        .map_or(READ, |most| most.min(READ_AT_MOST))
}

fn shown(bytes: &[u8], encoding: &str) -> Result<Value, String> {
    match encoding {
        "text" => Ok(Value::String(String::from_utf8_lossy(bytes).into_owned())),
        "base64" => Ok(Value::String(STANDARD.encode(bytes))),
        other => Err(format!("unknown encoding {other}; use text or base64")),
    }
}

fn failed(error: ModelError) -> String {
    match error {
        ModelError::Cancelled => ModelError::Cancelled.to_string(),
        error => reason(error),
    }
}

pub async fn request(http: &dyn Http, arguments: &Value, cancel: &CancellationToken) -> Result<Value, String> {
    let asked = Asked::read(arguments)?;
    let encoding = arguments.get("encoding").and_then(Value::as_str).unwrap_or("text");
    let response = http.send(asked.outgoing(false), cancel).await.map_err(failed)?;
    let (status, headers) = (response.status, headers_of(&response));
    let bytes = response.body.capped(cancel, most_of(arguments)).await.map_err(failed)?;
    Ok(json!({ "status": status, "headers": headers, "body": shown(&bytes, encoding)? }))
}

#[derive(Default)]
pub struct Streams {
    open: Mutex<HashMap<u64, Framed>>,
    next: AtomicU64,
}

impl std::fmt::Debug for Streams {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let open = self.open.lock().map(|open| open.len()).unwrap_or_default();
        f.debug_struct("Streams").field("open", &open).finish_non_exhaustive()
    }
}

fn handle(arguments: &Value) -> Result<u64, String> {
    match arguments {
        Value::Number(id) => id.as_u64(),
        other => other.get("id").and_then(Value::as_u64),
    }
    .ok_or_else(|| "the stream handle must be the id that open returned".to_owned())
}

impl Streams {
    fn count(&self) -> usize {
        self.open.lock().map(|open| open.len()).unwrap_or(OPEN_AT_MOST)
    }

    pub async fn open(&self, http: &dyn Http, arguments: &Value, cancel: &CancellationToken) -> Result<Value, String> {
        let framing = Framing::named(arguments.get("framing").and_then(Value::as_str).unwrap_or("sse"))?;
        if self.count() >= OPEN_AT_MOST {
            return Err(format!("at most {OPEN_AT_MOST} streams are open at once; close one first"));
        }
        let asked = Asked::read(arguments)?;
        let response = http.send(asked.outgoing(true), cancel).await.map_err(failed)?;
        let (status, headers) = (response.status, headers_of(&response));
        if !(200..300).contains(&status) {
            let bytes = response.body.capped(cancel, ERROR_READ).await.unwrap_or_default();
            let body = String::from_utf8_lossy(&bytes).into_owned();
            return Ok(json!({ "id": null, "status": status, "headers": headers, "body": body }));
        }
        let id = self.next.fetch_add(1, Ordering::Relaxed) + 1;
        self.open.lock().map_err(|_| "the streams are unavailable")?.insert(id, Framed::new(response.body, framing));
        Ok(json!({ "id": id, "status": status, "headers": headers }))
    }

    pub async fn next(&self, arguments: &Value, cancel: &CancellationToken) -> Result<Value, String> {
        let id = handle(arguments)?;
        let taken = self.open.lock().map_err(|_| "the streams are unavailable")?.remove(&id);
        let Some(mut framed) = taken else {
            return Err(format!("stream {id} is not open"));
        };
        let next = framed.next(cancel).await.map_err(failed)?;
        if next.is_some() {
            self.open.lock().map_err(|_| "the streams are unavailable")?.insert(id, framed);
        }
        Ok(next.unwrap_or(Value::Null))
    }

    pub fn close(&self, arguments: &Value) -> Result<Value, String> {
        let id = handle(arguments)?;
        let closed = self.open.lock().map_err(|_| "the streams are unavailable")?.remove(&id).is_some();
        Ok(Value::Bool(closed))
    }

    pub async fn call(
        &self,
        http: &dyn Http,
        name: &str,
        arguments: &Value,
        cancel: &CancellationToken,
    ) -> Result<Value, String> {
        match name {
            "request" => request(http, arguments, cancel).await,
            "stream.open" => self.open(http, arguments, cancel).await,
            "stream.next" => self.next(arguments, cancel).await,
            "stream.close" => self.close(arguments),
            other => Err(format!("http.{other} is not a function")),
        }
    }
}

#[cfg(test)]
#[path = "script_http_tests.rs"]
mod tests;
