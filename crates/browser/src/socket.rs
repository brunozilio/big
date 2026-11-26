use std::time::Duration;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use tokio::sync::mpsc::{UnboundedReceiver, unbounded_channel};

use crate::hub::{BrowserInfo, Hub};

pub const ROUTE: &str = "/api/browser/socket";

const KEEP_ALIVE: Duration = Duration::from_secs(20);
const LARGEST_MESSAGE: usize = 256 * 1024 * 1024;

pub fn connect(hub: Hub, upgrade: WebSocketUpgrade, origin: Option<&str>) -> Response {
    if !origin.is_some_and(|origin| origin.starts_with("chrome-extension://")) {
        return (StatusCode::FORBIDDEN, "only the browser extension connects here").into_response();
    }
    upgrade
        .max_message_size(LARGEST_MESSAGE)
        .max_frame_size(LARGEST_MESSAGE)
        .on_upgrade(move |socket| serve(hub, socket))
        .into_response()
}

async fn serve(hub: Hub, socket: WebSocket) {
    let (mut sink, mut stream) = socket.split();
    let Some(info) = hello(&mut stream).await else { return };
    let (outbox, inbox) = unbounded_channel();
    let own = outbox.clone();
    let connection = hub.link(info.clone(), outbox);
    tracing::info!(device = %info.device_id, name = %info.name, "a browser connected");
    let writer = tokio::spawn(async move { write(&mut sink, inbox).await });
    let pinger = tokio::spawn(async move {
        let mut every = tokio::time::interval(KEEP_ALIVE);
        let ping = serde_json::json!({ "type": "ping" }).to_string();
        loop {
            every.tick().await;
            if own.send(ping.clone()).is_err() {
                break;
            }
        }
    });
    while let Some(Ok(message)) = stream.next().await {
        match message {
            Message::Text(text) => receive(&hub, &info.device_id, text.as_str()),
            Message::Close(_) => break,
            _ => {}
        }
    }
    pinger.abort();
    writer.abort();
    hub.unlink(connection);
    tracing::info!(device = %info.device_id, "a browser disconnected");
}

async fn hello(stream: &mut futures_util::stream::SplitStream<WebSocket>) -> Option<BrowserInfo> {
    let first = tokio::time::timeout(Duration::from_secs(10), stream.next()).await.ok()??.ok()?;
    let Message::Text(text) = first else { return None };
    let hello: Value = serde_json::from_str(text.as_str()).ok()?;
    parse_hello(&hello)
}

fn parse_hello(hello: &Value) -> Option<BrowserInfo> {
    if hello["type"] != "hello" {
        return None;
    }
    let field = |name: &str| hello[name].as_str().unwrap_or_default().trim().to_owned();
    let device_id = field("deviceId");
    (!device_id.is_empty()).then(|| BrowserInfo {
        name: Some(field("name")).filter(|name| !name.is_empty()).unwrap_or_else(|| "Chrome".to_owned()),
        platform: field("platform"),
        version: field("version"),
        device_id,
    })
}

async fn write(sink: &mut futures_util::stream::SplitSink<WebSocket, Message>, mut inbox: UnboundedReceiver<String>) {
    while let Some(text) = inbox.recv().await {
        if sink.send(Message::Text(text.into())).await.is_err() {
            break;
        }
    }
}

fn receive(hub: &Hub, device: &str, text: &str) {
    let Ok(message) = serde_json::from_str::<Value>(text) else { return };
    let id = message["id"].as_u64().unwrap_or_default();
    match message["type"].as_str().unwrap_or_default() {
        "result" => {
            let answer = match message["error"].as_str() {
                Some(error) => Err(error.to_owned()),
                None => Ok(message["result"].clone()),
            };
            hub.answer(id, answer);
        }
        "paired" => hub.paired(id, device, message["name"].as_str().unwrap_or_default()),
        "rename" => hub.rename(device, message["name"].as_str().unwrap_or_default()),
        "captured" => {
            if let Some(flow) = message["flow"].as_str() {
                hub.captured(
                    flow,
                    json!({ "url": message["url"], "status": message["status"], "body": message["body"] }),
                );
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_hello_needs_a_device_id_and_gets_a_default_name() {
        assert_eq!(parse_hello(&json!({ "type": "hello" })), None);
        assert_eq!(parse_hello(&json!({ "type": "other", "deviceId": "a" })), None);
        let info = parse_hello(&json!({ "type": "hello", "deviceId": "a", "platform": "mac" }));
        assert_eq!(info.map(|info| (info.name, info.platform)), Some(("Chrome".to_owned(), "mac".to_owned())));
    }

    #[test]
    fn results_and_errors_reach_the_waiting_call() {
        let hub = Hub::new();
        let (outbox, mut inbox) = unbounded_channel();
        hub.link(
            BrowserInfo { device_id: "a".into(), name: "A".into(), platform: String::new(), version: String::new() },
            outbox,
        );
        let answering = hub.clone();
        let worker = std::thread::spawn(move || {
            let sent: Value = serde_json::from_str(&inbox.blocking_recv().unwrap_or_default()).unwrap_or_default();
            let reply = json!({ "type": "result", "id": sent["id"], "error": "boom" });
            receive(&answering, "a", &reply.to_string());
        });
        let answer = hub.call("a", json!({ "type": "call" }), Duration::from_secs(5));
        worker.join().unwrap_or_default();
        assert_eq!(answer, Err("boom".to_owned()));
    }
}
