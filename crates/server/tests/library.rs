#![allow(clippy::expect_used, reason = "a failed setup step stops the test")]

mod support;

use axum::http::{Method, StatusCode};
use serde_json::{Value, json};
use support::World;

#[tokio::test(flavor = "multi_thread")]
async fn the_library_lists_what_the_content_declares_and_opens_an_agent_as_a_form() {
    let world = World::new().await;
    let listed = world.get("/api/library").await;
    let keys: Vec<&str> =
        listed["sections"].as_array().into_iter().flatten().filter_map(|section| section["key"].as_str()).collect();
    assert_eq!(keys, ["agents", "skills", "prompts", "workflows", "tools", "styles", "scripts"]);
    let drafted =
        json!({ "path": "workflows/default.ts", "source": "steps: [\n    plan({ agent: \"planner\" }),\n  ],\n" });
    let (status, steps) = world.send(Method::POST, "/api/library/outline", Some(drafted)).await;
    assert_eq!((status, &steps[0]["agent"]), (StatusCode::OK, &json!("planner")));
    let opened = world.get("/api/library/item?path=agents/developer.ts").await;
    assert_eq!(opened["mode"], "form");
    assert_eq!(opened["fields"][0]["key"], "label");
    assert!(opened["assist"]["ideas"].as_array().is_some_and(|ideas| !ideas.is_empty()), "{opened}");
    let (status, _) = world.send(Method::GET, "/api/library/item?path=../config.toml", None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(opened["actions"], json!([]));
    let acted = json!({ "path": "agents/developer.ts", "action": "approve" });
    let (status, refused) = world.send(Method::POST, "/api/library/act", Some(acted)).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{refused}");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_chat_request_streams_back_the_changes_that_fit_the_item() {
    let world = World::new().await;
    let asked = json!({
        "path": "agents/developer.ts", "values": {}, "body": null, "source": null,
        "messages": [{ "role": "user", "text": "muda o nome" }],
    });
    let (status, told) = world.send(Method::POST, "/api/library/assist", Some(asked)).await;
    assert_eq!(status, StatusCode::OK, "{told}");
    let text = told.as_str().expect("a stream of events");
    let last = text.lines().filter_map(|line| line.strip_prefix("data:")).next_back().expect("an event");
    let done: Value = serde_json::from_str(last.trim()).expect("json");
    assert_eq!(done["type"], "done", "{text}");
    assert_eq!(done["reply"], "Feito.");
    assert_eq!(done["values"], json!({ "label": "Builder" }));
    let empty = json!({ "path": "agents/developer.ts", "messages": [] });
    let (status, _) = world.send(Method::POST, "/api/library/assist", Some(empty)).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}
