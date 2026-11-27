mod support;

use axum::http::{Method, StatusCode};
use serde_json::json;
use support::World;

#[tokio::test(flavor = "multi_thread")]
async fn running_locally_signs_in_the_local_user_without_a_login() {
    let world = World::new().await;
    let (status, body) = world.send(Method::GET, "/api/auth/me", None).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["mode"], "local");
    assert_eq!(body["user"]["user"], "local");
    let (status, _) = world.send(Method::GET, "/api/items", None).await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test(flavor = "multi_thread")]
async fn the_ways_of_signing_in_are_listed_without_a_session() {
    let world = World::new().await;
    let (status, body) = world.send(Method::GET, "/api/auth/methods", None).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(body["methods"].as_array().is_some_and(|methods| methods.contains(&json!("password"))), "{body}");
    assert!(!body["methods"].as_array().is_some_and(|methods| methods.contains(&json!("sso"))), "{body}");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_wrong_email_code_is_refused() {
    let world = World::new().await;
    let sent = json!({ "email": "ana@example.com", "code": "000000" });
    let (status, body) = world.send(Method::POST, "/api/auth/email/verify", Some(sent)).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
}
