use std::time::Duration;

use axum::Router;
use axum::extract::State;
use axum::extract::ws::WebSocketUpgrade;
use axum::http::HeaderMap;
use axum::routing::get;
use browser::{Hub, Session, Settings};
use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;

async fn route(State(hub): State<Hub>, headers: HeaderMap, upgrade: WebSocketUpgrade) -> axum::response::Response {
    let origin = headers.get("origin").and_then(|value| value.to_str().ok());
    browser::connect(hub, upgrade, origin)
}

async fn serve(hub: Hub) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap_or_else(|_| unreachable!());
    let address = listener.local_addr().unwrap_or_else(|_| unreachable!());
    let app = Router::new().route(browser::ROUTE, get(route)).with_state(hub);
    tokio::spawn(async move { axum::serve(listener, app).await });
    format!("ws://{address}{}", browser::ROUTE)
}

async fn wait_for_browser(hub: &Hub) {
    for _ in 0..100 {
        if !hub.browsers().is_empty() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn an_extension_connects_says_hello_and_answers_the_calls_of_a_session() {
    let hub = Hub::new();
    let url = serve(hub.clone()).await;
    let mut request = url.as_str().into_client_request().unwrap_or_else(|_| unreachable!());
    request.headers_mut().insert("origin", "chrome-extension://abc".parse().unwrap_or_else(|_| unreachable!()));
    let (mut socket, _) = tokio_tungstenite::connect_async(request).await.unwrap_or_else(|_| unreachable!());
    let hello = json!({ "type": "hello", "deviceId": "dev-1", "name": "Work Chrome", "platform": "mac", "version": "1" });
    socket.send(Message::Text(hello.to_string().into())).await.unwrap_or_default();
    wait_for_browser(&hub).await;
    assert_eq!(hub.browsers()[0].name, "Work Chrome");
    let extension = tokio::spawn(async move {
        while let Some(Ok(Message::Text(text))) = socket.next().await {
            let message: Value = serde_json::from_str(text.as_str()).unwrap_or_default();
            if message["type"] != "call" {
                continue;
            }
            let body = format!("{} in {}", message["tool"].as_str().unwrap_or_default(), message["session"].as_str().unwrap_or_default());
            let reply = json!({ "type": "result", "id": message["id"], "result": { "content": [{ "type": "text", "text": body }], "isError": false } });
            socket.send(Message::Text(reply.to_string().into())).await.unwrap_or_default();
            return;
        }
    });
    let folder = tempfile::tempdir().unwrap_or_else(|_| unreachable!());
    let settings = Settings { call: Duration::from_secs(5), pairing: Duration::from_secs(1), upload_limit: 1024, files: folder.path().to_path_buf() };
    let session = Session::new(hub.clone(), "chat-9", settings, vec![]);
    let asked = json!({ "jsonrpc": "2.0", "id": 4, "method": "tools/call", "params": { "name": "tabs_context_mcp", "arguments": {} } });
    let answer = tokio::task::spawn_blocking(move || domain::mcp::McpEndpoint::handle(&session, &asked)).await.unwrap_or_default().unwrap_or_default();
    assert_eq!(answer["result"]["content"][0]["text"], "tabs_context_mcp in chat-9");
    extension.await.unwrap_or_default();
}

#[tokio::test(flavor = "multi_thread")]
async fn a_client_that_is_not_an_extension_is_refused() {
    let url = serve(Hub::new()).await;
    let refused = tokio_tungstenite::connect_async(url.as_str()).await;
    assert!(refused.is_err());
}
