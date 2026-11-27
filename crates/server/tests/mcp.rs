#![allow(clippy::expect_used, reason = "a failed setup step stops the test")]

mod support;

use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use support::World;
use tower::ServiceExt;

const ROUTE: &str = "/mcp";

async fn call(
    world: &World,
    token: Option<&str>,
    session: Option<&str>,
    body: &Value,
) -> (StatusCode, Option<String>, Value) {
    asked(world, &[("mcp-resources", "browser")], token, session, body).await
}

async fn asked(
    world: &World,
    headers: &[(&str, &str)],
    token: Option<&str>,
    session: Option<&str>,
    body: &Value,
) -> (StatusCode, Option<String>, Value) {
    let mut request = Request::builder()
        .method(Method::POST)
        .uri(ROUTE)
        .header("host", "127.0.0.1:7999")
        .header("content-type", "application/json");
    for (name, value) in headers {
        request = request.header(*name, *value);
    }
    if let Some(token) = token {
        request = request.header("authorization", format!("Bearer {token}"));
    }
    if let Some(session) = session {
        request = request.header("mcp-session-id", session);
    }
    let request = request.body(Body::from(body.to_string())).expect("request");
    let response = world.router.clone().oneshot(request).await.expect("response");
    let status = response.status();
    let id = response.headers().get("mcp-session-id").and_then(|value| value.to_str().ok()).map(str::to_owned);
    let bytes = response.into_body().collect().await.expect("body").to_bytes();
    (status, id, serde_json::from_slice(&bytes).unwrap_or(Value::Null))
}

fn token(world: &World) -> String {
    let file = world.dir.path().join("data").join(brand::mcp::TOKEN_FILE.trim());
    std::fs::read_to_string(file).unwrap_or_default().trim().to_owned()
}

#[tokio::test(flavor = "multi_thread")]
async fn another_agent_reaches_the_browser_tools_with_the_token() {
    let world = World::new().await;
    assert_eq!(brand::mcp::ROUTE.trim(), ROUTE);
    assert_eq!(brand::mcp::RESOURCES_HEADER.trim(), "mcp-resources");
    let list = json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/list" });
    let (status, _, refused) = call(&world, Some("wrong"), None, &list).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "{refused}");
    let token = token(&world);
    assert_eq!(token.len(), 64);
    let initialize = json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": { "protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": { "name": "other", "version": "1" } } });
    let (status, session, hello) = call(&world, Some(&token), None, &initialize).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(hello["result"]["serverInfo"]["name"], json!(browser::server_name()));
    let session = session.expect("a session id");
    let notified = json!({ "jsonrpc": "2.0", "method": "notifications/initialized" });
    assert_eq!(call(&world, Some(&token), Some(&session), &notified).await.0, StatusCode::ACCEPTED);
    let (status, kept, tools) = call(&world, Some(&token), Some(&session), &list).await;
    assert_eq!((status, kept.as_deref()), (StatusCode::OK, Some(session.as_str())));
    assert_eq!(tools["result"]["tools"].as_array().map(Vec::len), Some(browser::tools().len()));
    let tabs = json!({ "jsonrpc": "2.0", "id": 3, "method": "tools/call", "params": { "name": "tabs_context_mcp", "arguments": {} } });
    let (_, _, answer) = call(&world, Some(&token), Some(&session), &tabs).await;
    assert_eq!(answer["result"]["isError"], json!(true), "{answer}");
    world.send(Method::PUT, "/api/browser", Some(json!({ "enabled": false }))).await;
    assert_eq!(call(&world, Some(&token), Some(&session), &list).await.0, StatusCode::SERVICE_UNAVAILABLE);
}

fn text(answer: &Value) -> String {
    answer["result"]["content"][0]["text"].as_str().unwrap_or_default().to_owned()
}

#[tokio::test(flavor = "multi_thread")]
async fn another_agent_reads_and_writes_memory_and_gets_only_the_resources_it_names() {
    let world = World::new().await;
    let list = json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/list" });
    assert_eq!(asked(&world, &[], Some("wrong"), None, &list).await.0, StatusCode::UNAUTHORIZED);
    let token = token(&world);
    let project = world.dir.path().display().to_string();
    let (status, _, refused) = asked(&world, &[], Some(&token), None, &list).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{refused}");
    let unknown = [("mcp-resources", "skills")];
    assert_eq!(asked(&world, &unknown, Some(&token), None, &list).await.0, StatusCode::BAD_REQUEST);
    let homeless = [("mcp-resources", "memory")];
    assert_eq!(asked(&world, &homeless, Some(&token), None, &list).await.0, StatusCode::BAD_REQUEST);
    let memory = [("mcp-resources", "memory"), ("mcp-project", project.as_str())];
    let (status, _, tools) = asked(&world, &memory, Some(&token), None, &list).await;
    assert_eq!(status, StatusCode::OK, "{tools}");
    assert_eq!(tools["result"]["tools"].as_array().map(Vec::len), Some(4));
    let remember = json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/call", "params": { "name": "memory_remember", "arguments": { "text": "Deploys go out on Tuesdays", "kind": "semantic" } } });
    let (_, _, kept) = asked(&world, &memory, Some(&token), None, &remember).await;
    assert!(text(&kept).starts_with("Remembered as ["), "{kept}");
    let search = json!({ "jsonrpc": "2.0", "id": 3, "method": "tools/call", "params": { "name": "memory_search", "arguments": { "query": "deploys" } } });
    let (_, _, found) = asked(&world, &memory, Some(&token), None, &search).await;
    assert!(text(&found).contains("Deploys go out on Tuesdays"), "{found}");
    let both = [("mcp-resources", "memory,browser"), ("mcp-project", project.as_str())];
    let (_, _, tools) = asked(&world, &both, Some(&token), None, &list).await;
    assert_eq!(tools["result"]["tools"].as_array().map(Vec::len), Some(4 + browser::tools().len()));
}
