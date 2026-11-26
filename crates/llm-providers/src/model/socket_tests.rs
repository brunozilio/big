use futures_util::{SinkExt, StreamExt};
use tokio::net::TcpListener;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::handshake::server::{ErrorResponse, Request, Response};

use super::{exchange, is_socket};
use crate::model::{CancellationToken, ModelError};

#[allow(clippy::result_large_err)]
fn checked(request: &Request, response: Response) -> Result<Response, ErrorResponse> {
    assert_eq!(request.headers().get("x-who").and_then(|value| value.to_str().ok()), Some("me"));
    Ok(response)
}

async fn served(answers: Vec<Message>) -> (String, tokio::task::JoinHandle<Vec<Message>>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap_or_else(|error| panic!("{error}"));
    let address = listener.local_addr().unwrap_or_else(|error| panic!("{error}"));
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap_or_else(|error| panic!("{error}"));
        let mut socket =
            tokio_tungstenite::accept_hdr_async(stream, checked).await.unwrap_or_else(|error| panic!("{error}"));
        let mut heard = Vec::new();
        for _ in 0..2 {
            if let Some(Ok(message)) = socket.next().await {
                heard.push(message);
            }
        }
        for answer in answers {
            socket.send(answer).await.unwrap_or_else(|error| panic!("{error}"));
        }
        socket.close(None).await.unwrap_or_else(|error| panic!("{error}"));
        heard
    });
    (format!("ws://{address}/chat"), server)
}

#[tokio::test]
async fn a_socket_sends_the_scripted_frames_and_streams_what_comes_back() {
    let (url, server) = served(vec![Message::Binary(b"ab".to_vec().into()), Message::Text("cd".into())]).await;
    let headers = vec![("x-who".to_owned(), "me".to_owned())];
    let body = r#"{"send":[{"text":"hello"},{"base64":"AQI="}]}"#;
    let cancel = CancellationToken::new();
    let response = exchange(&url, &headers, body, &cancel).await.unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(response.status, 200);
    let text = response.body.text(&cancel, 1024).await.unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(text, "abcd");
    let heard = server.await.unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(heard, vec![Message::Text("hello".into()), Message::Binary(vec![1_u8, 2].into())]);
}

#[tokio::test]
async fn frames_that_are_not_json_are_refused_before_connecting() {
    let cancel = CancellationToken::new();
    let refused = exchange("ws://127.0.0.1:9/never", &[], "nope", &cancel).await;
    assert!(matches!(refused, Err(ModelError::Provider(_))), "{refused:?}");
}

#[test]
fn only_websocket_urls_are_sockets() {
    assert!(is_socket("wss://host/path"));
    assert!(is_socket("ws://127.0.0.1:1"));
    assert!(!is_socket("https://host/path"));
}
