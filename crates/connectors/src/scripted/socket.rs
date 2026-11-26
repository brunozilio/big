use std::time::Duration;

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use futures_util::stream::{SplitSink, SplitStream};
use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use tokio::net::TcpStream;
use tokio::sync::Mutex;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::{HeaderName, HeaderValue};
use tokio_tungstenite::tungstenite::{Error as SocketError, Message};
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

use crate::error::ActionError;

const WAIT_AT_MOST: Duration = Duration::from_secs(300);
const OPEN_AT_MOST: usize = 8;

type Stream = WebSocketStream<MaybeTlsStream<TcpStream>>;

struct Held {
    sink: SplitSink<Stream, Message>,
    source: SplitStream<Stream>,
}

#[derive(Default)]
pub struct Sockets {
    open: Mutex<Vec<Option<Held>>>,
}

impl std::fmt::Debug for Sockets {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Sockets").finish_non_exhaustive()
    }
}

fn field<'a>(arguments: &'a Value, name: &str) -> &'a str {
    arguments[name].as_str().unwrap_or_default()
}

fn transient(error: impl std::fmt::Display) -> ActionError {
    ActionError::transient(error.to_string())
}

fn message(frame: &Value) -> Result<Message, ActionError> {
    if let Some(text) = frame["text"].as_str() {
        return Ok(Message::Text(text.to_owned().into()));
    }
    if let Some(binary) = frame["binary"].as_str() {
        return Ok(Message::Binary(binary.as_bytes().to_vec().into()));
    }
    let encoded = frame["base64"].as_str().ok_or_else(|| ActionError::permanent("a socket frame needs text, binary or base64"))?;
    let bytes = STANDARD.decode(encoded).map_err(|error| ActionError::permanent(error.to_string()))?;
    Ok(Message::Binary(bytes.into()))
}

fn request(arguments: &Value) -> Result<tokio_tungstenite::tungstenite::handshake::client::Request, ActionError> {
    let url = field(arguments, "url");
    if !url.starts_with("wss://") && !url.starts_with("ws://") {
        return Err(ActionError::permanent(format!("{url} is not a websocket address")));
    }
    let mut request = url.into_client_request().map_err(|error| ActionError::permanent(error.to_string()))?;
    for (name, value) in arguments["headers"].as_object().into_iter().flatten() {
        let name = HeaderName::from_bytes(name.as_bytes()).map_err(|error| ActionError::permanent(error.to_string()))?;
        let value = HeaderValue::from_str(value.as_str().unwrap_or_default())
            .map_err(|error| ActionError::permanent(error.to_string()))?;
        request.headers_mut().insert(name, value);
    }
    Ok(request)
}

fn index(arguments: &Value) -> usize {
    arguments["socket"].as_u64().and_then(|index| usize::try_from(index).ok()).unwrap_or(usize::MAX)
}

fn gone() -> ActionError {
    ActionError::permanent("that socket is not open")
}

impl Sockets {
    pub async fn open(&self, arguments: &Value) -> Result<Value, ActionError> {
        let request = request(arguments)?;
        let (stream, _) = match tokio_tungstenite::connect_async(request).await {
            Ok(pair) => pair,
            Err(SocketError::Http(refused)) => {
                let status = refused.status().as_u16();
                let body = String::from_utf8_lossy(&refused.into_body().unwrap_or_default()).into_owned();
                return Ok(json!({ "status": status, "body": body }));
            }
            Err(error) => return Err(transient(error)),
        };
        let (sink, source) = stream.split();
        let mut open = self.open.lock().await;
        if open.iter().filter(|held| held.is_some()).count() >= OPEN_AT_MOST {
            return Err(ActionError::permanent("too many sockets are open at once"));
        }
        open.push(Some(Held { sink, source }));
        Ok(json!({ "status": 101, "socket": open.len() - 1 }))
    }

    pub async fn send(&self, arguments: &Value) -> Result<Value, ActionError> {
        let frames = arguments["frames"].as_array().cloned().unwrap_or_default();
        let messages = frames.iter().map(message).collect::<Result<Vec<Message>, ActionError>>()?;
        let mut open = self.open.lock().await;
        let held = open.get_mut(index(arguments)).and_then(Option::as_mut).ok_or_else(gone)?;
        for message in messages {
            held.sink.send(message).await.map_err(transient)?;
        }
        Ok(json!({ "sent": frames.len() }))
    }

    pub async fn receive(&self, arguments: &Value) -> Result<Value, ActionError> {
        let wait = arguments["timeoutMs"].as_u64().map_or(WAIT_AT_MOST, Duration::from_millis).min(WAIT_AT_MOST);
        let mut open = self.open.lock().await;
        let held = open.get_mut(index(arguments)).and_then(Option::as_mut).ok_or_else(gone)?;
        loop {
            let next = match tokio::time::timeout(wait, held.source.next()).await {
                Ok(next) => next,
                Err(_) => return Ok(json!({ "timeout": true })),
            };
            match next {
                Some(Ok(Message::Binary(bytes))) => return Ok(json!({ "base64": STANDARD.encode(&bytes) })),
                Some(Ok(Message::Text(text))) => return Ok(json!({ "text": text.as_str() })),
                Some(Ok(Message::Close(_))) | None => return Ok(json!({ "closed": true })),
                Some(Ok(_)) => held.sink.flush().await.map_err(transient)?,
                Some(Err(error)) => return Err(transient(error)),
            }
        }
    }

    pub async fn close(&self, arguments: &Value) -> Result<Value, ActionError> {
        let mut open = self.open.lock().await;
        if let Some(mut held) = open.get_mut(index(arguments)).and_then(Option::take) {
            let _ = held.sink.send(Message::Close(None)).await;
        }
        Ok(Value::Null)
    }
}

#[cfg(test)]
#[path = "socket_tests.rs"]
mod tests;
