#![allow(clippy::expect_used, reason = "a failed setup step stops the test")]

mod support;

use std::time::Duration;

use axum::http::{Method, StatusCode};
use serde_json::{Value, json};
use support::{SESSION, World, fixture};

fn keys(value: &Value) -> Vec<String> {
    let mut keys: Vec<String> = value.as_object().map(|fields| fields.keys().cloned().collect()).unwrap_or_default();
    keys.sort();
    keys
}

const ITEM_KEYS: [&str; 17] = [
    "activity",
    "attention",
    "change",
    "createdAt",
    "current",
    "doneAt",
    "group",
    "id",
    "impediment",
    "issue",
    "live",
    "status",
    "steps",
    "title",
    "updatedAt",
    "workflow",
    "workflow",
];

fn item_keys() -> Vec<String> {
    let mut wanted: Vec<String> = ITEM_KEYS.iter().map(|key| (*key).to_owned()).collect();
    wanted.dedup();
    wanted.sort();
    wanted
}

#[tokio::test(flavor = "multi_thread")]
async fn an_item_runs_waits_for_the_human_and_is_delivered() {
    let world = World::new().await;
    let created = world
        .send(
            Method::POST,
            "/api/items",
            Some(json!({
                "text": "Corrigir o login\nDetalhes", "issue": null, "model": "auto", "attachments": [],
            })),
        )
        .await;
    assert_eq!(created.0, StatusCode::OK, "{}", created.1);
    let item = created.1;
    assert_eq!(keys(&item), item_keys());
    assert_eq!(item["title"], "Corrigir o login");
    assert_eq!(item["steps"][0]["label"], "Trabalho");
    let id = item["id"].as_str().expect("id").to_owned();

    let waiting = world.until("waiting", |item| item["group"] == "you").await;
    assert_eq!(waiting["attention"]["action"]["step"], "approve");
    assert_eq!(waiting["steps"][1]["state"], "waiting_human");
    assert_eq!(waiting["steps"][1]["logo"], "data:image/svg+xml;base64,PHN2Zy8+");
    assert_ne!(waiting["updatedAt"], item["updatedAt"]);
    fixture("item", &waiting);
    fixture("items", &world.get("/api/items").await);

    let work = world.get(&format!("/api/items/{id}/steps/work")).await;
    assert_eq!(work["blocks"], json!([{ "type": "chat", "session": SESSION, "composer": true }]));
    assert_eq!(work["attempts"][0]["state"], "passed");
    fixture("step", &work);
    let approve = world.get(&format!("/api/items/{id}/steps/approve")).await;
    assert_eq!(
        approve["blocks"][1],
        json!({ "type": "actions", "buttons": [{ "action": "approve", "label": "approve" }] })
    );
    fixture("step-waiting", &approve);

    let wrong =
        world.send(Method::POST, &format!("/api/items/{id}/steps/work/input"), Some(json!({ "payload": {} }))).await;
    assert_eq!(wrong.0, StatusCode::BAD_REQUEST);
    let answered = world
        .send(
            Method::POST,
            &format!("/api/items/{id}/steps/approve/input"),
            Some(json!({ "action": "approve", "payload": null })),
        )
        .await;
    assert_eq!(answered.0, StatusCode::NO_CONTENT, "{}", answered.1);

    let delivered = world.until("delivered", |item| item["group"] == "delivered").await;
    assert!(delivered["doneAt"].is_string());
    assert_eq!(delivered["issue"]["url"], "https://tracker.test/K-1", "the issue the step reported");
    assert_eq!(delivered["change"]["key"], "o/r#3");
    assert_eq!(delivered["change"]["connection"], "c1");
    assert!(world.provider.calls.lock().expect("calls").iter().any(|(action, _)| action == "tracker.issue"));

    let rerun = world.send(Method::POST, &format!("/api/items/{id}/rerun"), Some(json!({ "step": "approve" }))).await;
    assert_eq!(rerun.0, StatusCode::NO_CONTENT, "{}", rerun.1);
    world.until("waiting again", |item| item["group"] == "you").await;
    let cancelled = world.send(Method::POST, &format!("/api/items/{id}/cancel"), Some(json!({}))).await;
    assert_eq!(cancelled.0, StatusCode::NO_CONTENT);
    world.until("cancelled", |item| item["group"] == "delivered").await;
    let missing = world.send(Method::POST, "/api/items/nope/cancel", Some(json!({}))).await;
    assert_eq!(missing.0, StatusCode::NOT_FOUND);
    assert_eq!(missing.1["error"]["code"], "not_found");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_new_item_needs_a_text_or_an_issue_and_keeps_the_issue_and_model() {
    let world = World::new().await;
    let empty = world
        .send(
            Method::POST,
            "/api/items",
            Some(json!({ "text": " ", "issue": null, "model": "auto", "attachments": [] })),
        )
        .await;
    assert_eq!(empty.0, StatusCode::BAD_REQUEST);
    let body = json!({ "text": "", "issue": { "connection": "c1", "key": "K-1" }, "model": "m1", "attachments": [] });
    let (status, item) = world.send(Method::POST, "/api/items", Some(body)).await;
    assert_eq!(status, StatusCode::OK, "{item}");
    assert_eq!(item["title"], "K-1");
    assert_eq!(item["issue"]["key"], "K-1");
    assert_eq!(item["issue"]["connection"], "c1");
    assert_eq!(item["issue"]["name"], "Fake");
    world.until("the agent ran", |item| item["group"] == "you").await;
    let asked = world.engine.requests();
    assert!(asked.iter().any(|request| request.session.model.id == "m1"), "the item's model went to the agent");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_session_shows_its_events_and_refuses_unknown_answers() {
    let world = World::new().await;
    world
        .send(
            Method::POST,
            "/api/items",
            Some(json!({ "text": "x", "issue": null, "model": "auto", "attachments": [] })),
        )
        .await;
    world.until("the agent ran", |item| item["group"] == "you").await;
    let session = world.get(&format!("/api/sessions/{SESSION}")).await;
    assert_eq!(keys(&session), ["agents", "events", "id", "mode", "model", "status", "title"]);
    assert_eq!(session["status"], "idle");
    fixture("session", &session);
    let bad = world.send(Method::GET, "/api/sessions/..", None).await;
    assert_ne!(bad.0, StatusCode::OK);
    let unknown = world
        .send(
            Method::POST,
            &format!("/api/sessions/{SESSION}/respond"),
            Some(json!({
                "event": "nope", "response": { "kind": "permission", "decision": "allowed" },
            })),
        )
        .await;
    assert_eq!(unknown.0, StatusCode::NOT_FOUND);
    let stop = world.send(Method::POST, &format!("/api/sessions/{SESSION}/stop"), Some(json!({}))).await;
    assert_eq!(stop.0, StatusCode::CONFLICT);
    let sent = world
        .send(
            Method::POST,
            &format!("/api/sessions/{SESSION}/messages"),
            Some(json!({ "text": "continue", "model": null, "attachments": [] })),
        )
        .await;
    assert_eq!(sent.0, StatusCode::NO_CONTENT, "{}", sent.1);
    let missing = world.send(Method::GET, "/api/sessions/nao-existe", None).await;
    assert_eq!(missing.0, StatusCode::NOT_FOUND);
}

#[tokio::test(flavor = "multi_thread")]
async fn connections_list_sign_in_with_a_device_choose_an_account_and_disconnect() {
    let world = World::new().await;
    let listed = world.get("/api/connections").await;
    assert_eq!(listed["providers"][0]["key"], "fake");
    assert_eq!(listed["providers"][0]["signIn"], "device");
    assert_eq!(listed["providers"][0]["group"], "Tarefas");
    assert_eq!(
        listed["connections"][0],
        json!({
            "id": "c1", "provider": "fake", "label": "Fake", "account": "eu",
            "capabilities": ["tracker", "code", "pipelines"], "status": "ok",
        })
    );
    fixture("connections", &listed);
    let (status, started) =
        world.send(Method::POST, "/api/connections/fake/start", Some(json!({ "fields": {} }))).await;
    assert_eq!(status, StatusCode::OK, "{started}");
    assert_eq!((started["kind"].as_str(), started["code"].as_str()), (Some("device"), Some("ABCD-EFGH")));
    let flow = started["flow"].clone();
    let pending = world.send(Method::POST, "/api/connections/device/poll", Some(json!({ "flow": flow }))).await.1;
    assert_eq!(pending, json!({ "state": "pending" }));
    let choose = world.send(Method::POST, "/api/connections/device/poll", Some(json!({ "flow": flow }))).await.1;
    assert_eq!(choose["state"], "choose", "{choose}");
    assert_eq!(choose["choices"][1]["detail"], "b.test");
    let done =
        world.send(Method::POST, "/api/connections/device/poll", Some(json!({ "flow": flow, "choice": "b" }))).await.1;
    assert_eq!(done["state"], "done", "{done}");
    assert_eq!(done["connection"]["account"], "conta-b");
    let id = done["connection"]["id"].as_str().expect("id").to_owned();
    assert_eq!(world.get("/api/connections").await["connections"].as_array().map(Vec::len), Some(2));
    let gone = world.send(Method::DELETE, &format!("/api/connections/{id}"), None).await;
    assert_eq!(gone.0, StatusCode::NO_CONTENT);
    assert_eq!(world.get("/api/connections").await["connections"].as_array().map(Vec::len), Some(1));
    let expired = world.send(Method::POST, "/api/connections/device/poll", Some(json!({ "flow": "x" }))).await.1;
    assert_eq!(expired["state"], "failed");
    assert_eq!(
        world.send(Method::POST, "/api/connections/nope/start", Some(json!({ "fields": {} }))).await.0,
        StatusCode::NOT_FOUND
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn the_page_s_reads_go_through_the_connection() {
    let world = World::new().await;
    let call = |action: &str, args: Value| json!({ "action": action, "args": args });
    let issue = world
        .send(Method::POST, "/api/connections/c1/call", Some(call("tracker.issue", json!({ "key": "K-1" }))))
        .await;
    assert_eq!(issue.0, StatusCode::OK, "{}", issue.1);
    assert_eq!(issue.1["url"], "https://tracker.test/K-1");
    fixture("call-issue", &issue.1);
    let found = world
        .send(Method::POST, "/api/connections/c1/call", Some(call("tracker.search", json!({ "query": "login" }))))
        .await
        .1;
    assert_eq!(found, json!([{ "key": "K-1", "title": "Corrigir", "status": "A fazer" }]));
    let comment = world
        .send(
            Method::POST,
            "/api/connections/c1/call",
            Some(call("tracker.comment", json!({ "key": "K-1", "body": "oi" }))),
        )
        .await
        .1;
    assert_eq!(comment["kind"], "comment");
    let change = world
        .send(Method::POST, "/api/connections/c1/call", Some(call("code.change", json!({ "change": "o/r#3" }))))
        .await
        .1;
    assert_eq!(change["files"][0]["path"], "a.rs");
    fixture("call-change", &change);
    let run =
        world.send(Method::POST, "/api/connections/c1/call", Some(call("ci.run", json!({ "run": "o/r#9" })))).await.1;
    assert_eq!(run["state"], "passed");
    let unknown = world.send(Method::POST, "/api/connections/c1/call", Some(call("x.y", json!({})))).await;
    assert_eq!(unknown.0, StatusCode::BAD_REQUEST);
    let missing =
        world.send(Method::POST, "/api/connections/zz/call", Some(call("tracker.issue", json!({ "key": "K" })))).await;
    assert_eq!(missing.0, StatusCode::NOT_FOUND);
}

#[tokio::test(flavor = "multi_thread")]
async fn models_branding_and_attachments() {
    let world = World::new().await;
    let models = world.get("/api/models").await;
    assert_eq!(models["default"], "auto");
    assert_eq!(models["models"][0]["id"], "auto");
    assert_eq!(models["models"][1]["id"], "m1");
    fixture("models", &models);
    let branding = world.get("/api/branding").await;
    assert_eq!(branding, json!({ "name": brand::NAME, "logo": null, "theme": "system" }));
    let (status, saved) = world
        .send(Method::PUT, "/api/branding", Some(json!({ "theme": "dark", "logo": "data:image/png;base64,AA==" })))
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(saved["theme"], "dark");
    fixture("branding", &world.get("/api/branding").await);
    let bad = world.send(Method::PUT, "/api/branding", Some(json!({ "theme": "rosa" }))).await;
    assert_eq!(bad.0, StatusCode::BAD_REQUEST);

    let boundary = "fronteira";
    let body = format!(
        "--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"nota.txt\"\r\nContent-Type: text/plain\r\n\r\nolá\r\n--{boundary}--\r\n"
    );
    let request = axum::http::Request::builder()
        .method(Method::POST)
        .uri("/api/attachments")
        .header("content-type", format!("multipart/form-data; boundary={boundary}"))
        .body(axum::body::Body::from(body))
        .expect("request");
    let response = tower::ServiceExt::oneshot(world.router.clone(), request).await.expect("response");
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = http_body_util::BodyExt::collect(response.into_body()).await.expect("body").to_bytes();
    let uploaded: Value = serde_json::from_slice(&bytes).expect("json");
    assert_eq!((uploaded["name"].as_str(), uploaded["size"].as_u64()), (Some("nota.txt"), Some(4)));
    let with_file = json!({ "text": "com anexo", "issue": null, "model": "auto", "attachments": [uploaded["id"]] });
    assert_eq!(world.send(Method::POST, "/api/items", Some(with_file)).await.0, StatusCode::OK);
}

#[tokio::test(flavor = "multi_thread")]
async fn the_page_is_served_for_every_route_of_the_router_and_unknown_api_paths_are_json() {
    let world = World::new().await;
    for path in ["/", "/items/abc", "/items/abc/plan", "/settings/connections"] {
        let (status, body) = world.send(Method::GET, path, None).await;
        assert_eq!(status, StatusCode::OK, "{path}");
        assert!(body.as_str().is_some_and(|page| page.contains("<title>page</title>")), "{path}: {body}");
    }
    let (status, script) = world.send(Method::GET, "/assets/app.js", None).await;
    assert_eq!((status, script.as_str()), (StatusCode::OK, Some("console.log(1)")));
    let (status, body) = world.send(Method::GET, "/api/nothing", None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["error"]["code"], "not_found");
    let (status, _) = world.send(Method::GET, "/api/connections/oauth/callback?state=x&code=y", None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test(flavor = "multi_thread")]
async fn the_live_feed_tells_about_items_sessions_connections_and_branding() {
    let world = World::new().await;
    let mut feed = world.ctx_live.subscribe();
    world
        .send(
            Method::POST,
            "/api/items",
            Some(json!({ "text": "ao vivo", "issue": null, "model": "auto", "attachments": [] })),
        )
        .await;
    world.until("waiting", |item| item["group"] == "you").await;
    world.send(Method::PUT, "/api/branding", Some(json!({ "theme": "light" }))).await;
    let mut seen: Vec<Value> = Vec::new();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    while tokio::time::Instant::now() < deadline {
        match tokio::time::timeout(Duration::from_millis(200), feed.recv()).await {
            Ok(Ok(text)) => seen.push(serde_json::from_str(&text).expect("json")),
            _ if seen.iter().any(|event| event["type"] == "branding")
                && seen.iter().any(|event| event["type"] == "session.status") =>
            {
                break;
            }
            _ => {}
        }
    }
    let types: Vec<&str> = seen.iter().filter_map(|event| event["type"].as_str()).collect();
    assert!(types.contains(&"item"), "{types:?}");
    assert!(types.contains(&"branding"), "{types:?}");
    assert!(types.contains(&"session.status"), "{types:?}");
    let first_item = seen.iter().find(|event| event["type"] == "item").cloned().expect("item event");
    fixture("live-item", &first_item);
    let status = seen.iter().find(|event| event["type"] == "session.status").cloned().expect("status");
    fixture("live-session-status", &status);
    let request = axum::http::Request::builder().uri("/api/live").body(axum::body::Body::empty()).expect("request");
    let response = tower::ServiceExt::oneshot(world.router.clone(), request).await.expect("response");
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()["content-type"], "text/event-stream");
}
