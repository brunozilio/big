use futures_util::{SinkExt, StreamExt};
use serde_json::json;
use tokio::net::TcpListener;
use tokio_tungstenite::tungstenite::Message;

use super::Sockets;

async fn echoing() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap_or_else(|error| panic!("{error}"));
    let address = listener.local_addr().unwrap_or_else(|error| panic!("{error}"));
    tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap_or_else(|error| panic!("{error}"));
        let mut socket = tokio_tungstenite::accept_async(stream).await.unwrap_or_else(|error| panic!("{error}"));
        while let Some(Ok(message)) = socket.next().await {
            let reply = match message {
                Message::Text(text) => Message::Text(format!("echo {}", text.as_str()).into()),
                Message::Binary(bytes) => Message::Binary(bytes),
                _ => break,
            };
            if socket.send(reply).await.is_err() {
                break;
            }
        }
    });
    format!("ws://{address}/chat")
}

#[tokio::test]
async fn a_socket_sends_frames_and_receives_them_one_at_a_time() {
    let sockets = Sockets::default();
    let opened = sockets.open(&json!({ "url": echoing().await, "headers": { "x-who": "me" } })).await;
    let Ok(opened) = opened else { panic!("{opened:?}") };
    assert_eq!(opened["status"], 101);
    let socket = opened["socket"].clone();
    let sent = sockets
        .send(&json!({ "socket": socket, "frames": [{ "text": "hi" }, { "base64": "AQI=" }, { "binary": "ok" }] }))
        .await;
    assert_eq!(sent.map(|sent| sent["sent"].clone()), Ok(json!(3)));
    let first = sockets.receive(&json!({ "socket": socket, "timeoutMs": 2000 })).await;
    assert_eq!(first, Ok(json!({ "text": "echo hi" })));
    let second = sockets.receive(&json!({ "socket": socket, "timeoutMs": 2000 })).await;
    assert_eq!(second, Ok(json!({ "base64": "AQI=" })));
    let third = sockets.receive(&json!({ "socket": socket, "timeoutMs": 2000 })).await;
    assert_eq!(third, Ok(json!({ "base64": "b2s=" })));
    let quiet = sockets.receive(&json!({ "socket": socket, "timeoutMs": 50 })).await;
    assert_eq!(quiet, Ok(json!({ "timeout": true })));
    assert!(sockets.close(&json!({ "socket": socket })).await.is_ok());
    assert!(sockets.send(&json!({ "socket": socket, "frames": [] })).await.is_err(), "a closed socket takes nothing");
}

#[tokio::test]
async fn only_websocket_addresses_are_opened() {
    let refused = Sockets::default().open(&json!({ "url": "https://example.com" })).await;
    assert!(refused.is_err());
}
