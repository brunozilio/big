use std::time::Duration;

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use futures_util::stream::{SplitSink, SplitStream};
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::{HeaderName, HeaderValue};
use tokio_tungstenite::tungstenite::{Error as SocketError, Message};
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

use super::body::{Body, Chunk};
use super::http::{Response, stream_idle};
use super::{CancellationToken, ModelError};

pub const KEEPALIVE: Duration = Duration::from_secs(25);
const QUEUE: usize = 64;

type Socket = WebSocketStream<MaybeTlsStream<TcpStream>>;
type Sink = SplitSink<Socket, Message>;
type Source = SplitStream<Socket>;

#[derive(Debug, Default, Deserialize)]
struct Outgoing {
    #[serde(default)]
    send: Vec<Frame>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum Frame {
    Text { text: String },
    Binary { binary: String },
    Base64 { base64: String },
}

impl Frame {
    fn message(self) -> Result<Message, ModelError> {
        match self {
            Frame::Text { text } => Ok(Message::Text(text.into())),
            Frame::Binary { binary } => Ok(Message::Binary(binary.into_bytes().into())),
            Frame::Base64 { base64 } => STANDARD
                .decode(base64)
                .map(|bytes| Message::Binary(bytes.into()))
                .map_err(|error| ModelError::Provider(format!("a socket frame is not base64: {error}"))),
        }
    }
}

pub fn is_socket(url: &str) -> bool {
    url.starts_with("wss://") || url.starts_with("ws://")
}

fn network(error: impl std::fmt::Display) -> ModelError {
    ModelError::Network(error.to_string())
}

fn frames(body: &str) -> Result<Vec<Message>, ModelError> {
    let outgoing: Outgoing = serde_json::from_str(body)
        .map_err(|error| ModelError::Provider(format!("the socket frames are not JSON: {error}")))?;
    outgoing.send.into_iter().map(Frame::message).collect()
}

fn request(
    url: &str,
    headers: &[(String, String)],
) -> Result<tokio_tungstenite::tungstenite::handshake::client::Request, ModelError> {
    let mut request = url.into_client_request().map_err(network)?;
    for (name, value) in headers {
        let name = HeaderName::from_bytes(name.as_bytes()).map_err(network)?;
        let value = HeaderValue::from_str(value).map_err(network)?;
        request.headers_mut().insert(name, value);
    }
    Ok(request)
}

fn listed(headers: &tokio_tungstenite::tungstenite::http::HeaderMap) -> Vec<(String, String)> {
    headers
        .iter()
        .filter_map(|(name, value)| value.to_str().ok().map(|value| (name.as_str().to_owned(), value.to_owned())))
        .collect()
}

pub async fn exchange(
    url: &str,
    headers: &[(String, String)],
    body: &str,
    cancel: &CancellationToken,
) -> Result<Response, ModelError> {
    let messages = frames(body)?;
    let request = request(url, headers)?;
    let connected = tokio::select! {
        biased;
        () = cancel.cancelled() => return Err(ModelError::Cancelled),
        connected = tokio_tungstenite::connect_async(request) => connected,
    };
    let (socket, answer) = match connected {
        Ok(pair) => pair,
        Err(SocketError::Http(refused)) => {
            let status = refused.status().as_u16();
            let headers = listed(refused.headers());
            let text = refused.into_body().unwrap_or_default();
            return Ok(Response { status, headers, body: Body::from(text) });
        }
        Err(error) => return Err(network(error)),
    };
    let headers = listed(answer.headers());
    let (mut sink, source) = socket.split();
    for message in messages {
        sink.send(message).await.map_err(network)?;
    }
    let (chunks, body) = mpsc::channel::<Chunk>(QUEUE);
    tokio::spawn(pump(sink, source, chunks, cancel.clone()));
    Ok(Response { status: 200, headers, body: Body::chunks(body).idle(stream_idle()) })
}

async fn pump(mut sink: Sink, mut source: Source, chunks: mpsc::Sender<Chunk>, cancel: CancellationToken) {
    let mut beat = tokio::time::interval(KEEPALIVE);
    beat.tick().await;
    loop {
        tokio::select! {
            () = cancel.cancelled() => break,
            () = chunks.closed() => break,
            _ = beat.tick() => {
                if sink.send(Message::Ping(Vec::new().into())).await.is_err() {
                    break;
                }
            }
            next = source.next() => {
                let chunk = match next {
                    Some(Ok(Message::Binary(bytes))) => Ok(bytes.to_vec()),
                    Some(Ok(Message::Text(text))) => Ok(text.as_str().as_bytes().to_vec()),
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Ok(_)) => {
                        if sink.flush().await.is_err() {
                            break;
                        }
                        continue;
                    }
                    Some(Err(error)) => Err(error.to_string()),
                };
                let failed = chunk.is_err();
                if chunks.send(chunk).await.is_err() || failed {
                    break;
                }
            }
        }
    }
    let _ = sink.send(Message::Close(None)).await;
}

#[cfg(test)]
#[path = "socket_tests.rs"]
mod tests;
