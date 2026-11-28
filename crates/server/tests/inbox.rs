#![allow(clippy::expect_used, reason = "a failed setup step stops the test")]

mod support;

use std::sync::Arc;
use std::time::Duration;

use axum::http::{Method, StatusCode};
use domain::permission::{Allowed, Behavior, Grant, Rule, allowed};
use serde_json::{Value, json};
use support::World;

fn asking_before_the_issue_is_read() {
    let tool = domain::mcp::served_name(
        &domain::mcp::connectors_server(),
        &domain::mcp::connector_tool("Fake", "tracker.issue"),
    );
    let list = Allowed::of([Rule::new(Grant::Tool(tool), Behavior::Ask)]);
    allowed::install(list, Arc::new(|_: &Allowed| {}));
}

async fn inbox_until(world: &World, what: &str, check: impl Fn(&[Value]) -> bool) -> Vec<Value> {
    for _ in 0..400 {
        let found = world.get("/api/inbox").await.as_array().cloned().unwrap_or_default();
        if check(&found) {
            return found;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!("never reached {what}: {}", world.get("/api/inbox").await)
}

async fn created(world: &World, text: &str) -> String {
    let made = json!({ "text": text, "issue": null, "model": "auto", "attachments": [] });
    let (status, item) = world.send(Method::POST, "/api/items", Some(made)).await;
    assert_eq!(status, StatusCode::OK, "{item}");
    item["id"].as_str().expect("id").to_owned()
}

fn of<'a>(found: &'a [Value], item: &str, kind: &str) -> Option<&'a Value> {
    found.iter().find(|entry| entry["item"]["id"] == item && entry["kind"] == kind)
}

#[tokio::test(flavor = "multi_thread")]
async fn a_review_and_a_permission_from_two_items_wait_together_and_are_answered_from_the_inbox() {
    let world = World::new().await;
    asking_before_the_issue_is_read();
    let first = created(&world, "Corrigir o login").await;
    let second = created(&world, "Trocar o logo").await;
    let both = |found: &[Value]| of(found, &first, "notice").is_some() && of(found, &second, "notice").is_some();
    let found = inbox_until(&world, "both reviews", both).await;
    let review = of(&found, &first, "notice").expect("review");
    assert_eq!(review["item"]["title"], "Corrigir o login");
    assert_eq!(review["item"]["step"], "approve");
    assert_eq!(review["item"]["stepLabel"], "Aprovação");
    assert_eq!(review["item"]["open"], first.as_str());
    assert_eq!(review["blocks"][1]["type"], "actions");
    let path = format!("/api/items/{first}/steps/approve/input");
    let (status, body) = world.send(Method::POST, &path, Some(json!({ "action": "approve", "payload": null }))).await;
    assert_eq!(status, StatusCode::NO_CONTENT, "{body}");
    let asked = |found: &[Value]| of(found, &first, "permission").is_some() && of(found, &second, "notice").is_some();
    let found = inbox_until(&world, "the permission next to the other review", asked).await;
    let permission = of(&found, &first, "permission").expect("permission");
    assert!(permission["session"].is_null(), "{permission}");
    assert_eq!(permission["event"]["type"], "permission");
    assert_eq!(permission["event"]["resolution"], "pending");
    let path = "/api/inbox/respond";
    let ask = permission["id"].clone();
    let wrong = json!({ "ask": ask, "response": { "kind": "ask", "answers": [] } });
    assert_eq!(world.send(Method::POST, path, Some(wrong)).await.0, StatusCode::BAD_REQUEST);
    let allowed = json!({ "ask": ask, "response": { "kind": "permission", "decision": "allowed" } });
    let (status, body) = world.send(Method::POST, path, Some(allowed.clone())).await;
    assert_eq!(status, StatusCode::NO_CONTENT, "{body}");
    world.until("delivered", |item| item["id"] == first.as_str() && item["group"] == "delivered").await;
    let found = world.get("/api/inbox").await.as_array().cloned().unwrap_or_default();
    assert!(found.iter().all(|entry| entry["item"]["id"] != first.as_str()), "{found:?}");
    assert!(of(&found, &second, "notice").is_some(), "{found:?}");
    assert_eq!(world.send(Method::POST, path, Some(allowed)).await.0, StatusCode::NOT_FOUND);
}
