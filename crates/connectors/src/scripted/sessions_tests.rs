use super::*;
use crate::http::{FakeHttp, HttpResponse, Method};

fn initialized(id: u64, session: &str) -> HttpResponse {
    HttpResponse::new(200, format!(r#"{{"jsonrpc":"2.0","id":{id},"result":{{}}}}"#))
        .with_header("mcp-session-id", session)
}

fn answered(id: u64, text: &str) -> HttpResponse {
    let body = json!({"jsonrpc": "2.0", "id": id, "result": {"content": [{"type": "text", "text": text}]}});
    HttpResponse::new(200, body.to_string())
}

fn identity(connection: &str, token: &str) -> Identity {
    Identity { connection: connection.to_owned(), url: "https://h/mcp".to_owned(), token: token.to_owned() }
}

fn methods(http: &FakeHttp) -> Vec<String> {
    http.sent().iter().map(|sent| sent.body_json()["method"].as_str().unwrap_or_default().to_owned()).collect()
}

fn call(tool: &str) -> Ask {
    Ask::Call(tool.to_owned(), json!({}))
}

#[tokio::test]
async fn a_second_call_reuses_the_session_without_initializing_again() {
    let http = FakeHttp::new()
        .respond(Method::Post, "/mcp", initialized(1, "s1"))
        .once(Method::Post, "/mcp", 202, "")
        .respond(Method::Post, "/mcp", answered(2, "a"))
        .respond(Method::Post, "/mcp", answered(3, "b"));
    let sessions = McpSessions::default();
    assert_eq!(sessions.ask(&http, &identity("c1", "t"), &call("x")).await, Ok(json!("a")));
    assert_eq!(sessions.ask(&http, &identity("c1", "t"), &call("x")).await, Ok(json!("b")));
    assert_eq!(methods(&http), ["initialize", "notifications/initialized", "tools/call", "tools/call"]);
    assert_eq!(sessions.len(), 1);
}

#[tokio::test]
async fn another_connection_or_token_opens_its_own_session() {
    let http = FakeHttp::new()
        .respond(Method::Post, "/mcp", initialized(1, "s1"))
        .once(Method::Post, "/mcp", 202, "")
        .respond(Method::Post, "/mcp", initialized(1, "s2"))
        .once(Method::Post, "/mcp", 202, "");
    let sessions = McpSessions::default();
    assert!(sessions.ask(&http, &identity("c1", "t"), &Ask::Connect).await.is_ok());
    assert!(sessions.ask(&http, &identity("c2", "t"), &Ask::Connect).await.is_ok());
    assert_eq!(methods(&http).iter().filter(|method| *method == "initialize").count(), 2);
    assert_eq!(sessions.len(), 2);
}

#[tokio::test]
async fn an_expired_session_is_opened_again_and_the_call_retried_once() {
    let http = FakeHttp::new()
        .respond(Method::Post, "/mcp", initialized(1, "s1"))
        .once(Method::Post, "/mcp", 202, "")
        .respond(Method::Post, "/mcp", answered(2, "a"))
        .once(Method::Post, "/mcp", 404, "session not found")
        .respond(Method::Post, "/mcp", initialized(1, "s2"))
        .once(Method::Post, "/mcp", 202, "")
        .respond(Method::Post, "/mcp", answered(2, "b"));
    let sessions = McpSessions::default();
    assert_eq!(sessions.ask(&http, &identity("c1", "t"), &call("x")).await, Ok(json!("a")));
    assert_eq!(sessions.ask(&http, &identity("c1", "t"), &call("x")).await, Ok(json!("b")));
    let sent = http.sent();
    assert_eq!(sent[6].header_value("mcp-session-id"), Some("s2"));
    assert_eq!(sessions.len(), 1);
}

#[tokio::test]
async fn a_fresh_session_refused_is_not_retried_and_not_kept() {
    let http = FakeHttp::new()
        .respond(Method::Post, "/mcp", initialized(1, "s1"))
        .once(Method::Post, "/mcp", 202, "")
        .once(Method::Post, "/mcp", 401, "invalid_token");
    let sessions = McpSessions::default();
    let refused = sessions.ask(&http, &identity("c1", "t"), &call("x")).await;
    assert_eq!(refused.map_err(|error| error.class), Err(crate::error::ErrorClass::Auth));
    assert_eq!(http.sent().len(), 3);
    assert_eq!(sessions.len(), 0);
}

#[tokio::test]
async fn an_idle_session_is_evicted() {
    let http = FakeHttp::new()
        .respond(Method::Post, "/mcp", initialized(1, "s1"))
        .once(Method::Post, "/mcp", 202, "")
        .respond(Method::Post, "/mcp", initialized(1, "s2"))
        .once(Method::Post, "/mcp", 202, "");
    let sessions = McpSessions::default();
    assert!(sessions.ask(&http, &identity("c1", "t"), &Ask::Connect).await.is_ok());
    sessions.age(IDLE);
    assert!(sessions.ask(&http, &identity("c1", "t"), &Ask::Connect).await.is_ok());
    assert_eq!(methods(&http).iter().filter(|method| *method == "initialize").count(), 2);
    assert_eq!(sessions.len(), 1);
}
