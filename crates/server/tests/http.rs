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

const ITEM_KEYS: [&str; 20] = [
    "activity",
    "attention",
    "change",
    "createdAt",
    "current",
    "doneAt",
    "git",
    "group",
    "id",
    "impediment",
    "issue",
    "live",
    "recap",
    "sources",
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
    assert_eq!(waiting["steps"][1]["logo"], "<svg/>");
    assert!(waiting["steps"][1]["needs"].is_string(), "{waiting}");
    assert!(waiting["steps"][1]["provider"].is_string(), "{waiting}");
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
    for wrong in [json!({ "effort": "enorme" }), json!({ "effort": "high", "fast": true })] {
        let mut body = json!({ "text": "x", "issue": null, "model": "m1", "attachments": [] });
        body.as_object_mut().expect("object").extend(wrong.as_object().expect("object").clone());
        assert_eq!(world.send(Method::POST, "/api/items", Some(body)).await.0, StatusCode::BAD_REQUEST);
    }
    let body = json!({
        "text": "", "issue": { "connection": "c1", "key": "K-1" }, "model": "m1", "effort": "high", "fast": false,
        "attachments": [],
    });
    let (status, item) = world.send(Method::POST, "/api/items", Some(body)).await;
    assert_eq!(status, StatusCode::OK, "{item}");
    assert_eq!(item["title"], "K-1");
    assert_eq!(item["issue"]["key"], "K-1");
    assert_eq!(item["issue"]["connection"], "c1");
    assert_eq!(item["issue"]["name"], "Fake");
    assert_eq!(item["sources"][0]["kind"], "issue");
    assert_eq!(item["sources"][0]["key"], "K-1");
    assert_eq!(item["sources"][0]["connection"], "c1");
    world.until("the agent ran", |item| item["group"] == "you").await;
    let asked = world.engine.requests();
    assert!(asked.iter().any(|request| request.session.model.id == "m1"), "the item's model went to the agent");
    assert!(asked.iter().any(|request| request.session.effort == domain::Effort::High), "and its effort");
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
    assert_eq!(keys(&session), ["agents", "events", "id", "mode", "model", "status", "suggestion", "title", "work"]);
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
    let mut feed = world.ctx_live.subscribe();
    let sent = world
        .send(
            Method::POST,
            &format!("/api/sessions/{SESSION}/messages"),
            Some(json!({ "text": "continue", "model": null, "attachments": [] })),
        )
        .await;
    assert_eq!(sent.0, StatusCode::NO_CONTENT, "{}", sent.1);
    let resumed = |request: &domain::PromptRequest| request.prompt.text == "continue";
    for _ in 0..200 {
        if world.engine.requests().iter().any(resumed) {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    let requests = world.engine.requests();
    let continued = requests.iter().find(|request| resumed(request)).expect("the message reached the agent");
    let first = requests.iter().find_map(|request| request.session.agent.as_ref()).map(|agent| agent.name.clone());
    assert!(first.is_some(), "the step ran a named agent");
    assert_eq!(
        continued.session.agent.as_ref().map(|agent| agent.name.clone()),
        first,
        "the chat goes on as the agent that ran the step, with its tools and mode"
    );
    let mut suggested = Value::Null;
    for _ in 0..200 {
        suggested = world.get(&format!("/api/sessions/{SESSION}")).await["suggestion"].clone();
        if !suggested.is_null() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    assert_eq!(suggested, "rode os testes", "a finished reply leaves a suggestion for what to say next");
    let mut announced = None;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    while announced.is_none() && tokio::time::Instant::now() < deadline {
        if let Ok(Ok(text)) = tokio::time::timeout(Duration::from_millis(200), feed.recv()).await {
            let event: Value = serde_json::from_str(&text).expect("json");
            if event["type"] == "session.suggestion" && !event["suggestion"].is_null() {
                announced = Some(event);
            }
        }
    }
    let announced = announced.expect("the suggestion is announced on the live feed");
    assert_eq!(announced["suggestion"], "rode os testes");
    fixture("live-session-suggestion", &announced);
    let side = world
        .send(Method::POST, &format!("/api/sessions/{SESSION}/side"), Some(json!({ "text": "em que passo está?" })))
        .await;
    assert_eq!(side.0, StatusCode::OK, "{}", side.1);
    assert_eq!(side.1["answer"], "está no passo dois");
    let asked_aside =
        world.engine.requests().into_iter().rfind(|r| r.session.command == Some(domain::SessionCommand::Side));
    let asked_aside = asked_aside.expect("the side question went to the agent's session");
    assert_eq!(asked_aside.session.resume.as_ref().map(|s| s.0.as_str()), Some(SESSION));
    let empty = world.send(Method::POST, &format!("/api/sessions/{SESSION}/side"), Some(json!({ "text": " " }))).await;
    assert_eq!(empty.0, StatusCode::BAD_REQUEST);
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
    let projects =
        world.send(Method::POST, "/api/connections/c1/call", Some(call("tracker.projects", json!({})))).await;
    assert_eq!(projects.0, StatusCode::OK, "{}", projects.1);
    assert_eq!(
        projects.1[0],
        json!({ "id": "1", "key": "APP", "name": "Aplicativo", "avatar": "https://tracker.test/app.png" })
    );
    assert_eq!(projects.1[1]["avatar"], Value::Null);
    fixture("call-projects", &projects.1);
    let filtered = call("tracker.search", json!({ "query": "", "project": "OPS" }));
    assert_eq!(world.send(Method::POST, "/api/connections/c1/call", Some(filtered)).await.0, StatusCode::OK);
    let sent = world.provider.calls.lock().expect("calls").clone();
    assert!(sent.iter().any(|(action, args)| action == "tracker.search" && args["project"] == "OPS"));
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
    let run = world
        .send(Method::POST, "/api/connections/c1/call", Some(call("ci.run", json!({ "run": "o/r/runs/9" }))))
        .await
        .1;
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
    assert_eq!(models["default"], "m1");
    assert_eq!(models["models"][0]["id"], "m1");
    assert_eq!(models["models"][0]["efforts"], json!([]));
    assert_eq!(models["models"][0]["fast"], false);
    fixture("models", &models);
    let branding = world.get("/api/branding").await;
    assert_eq!(branding, json!({ "name": brand::NAME, "logo": "/api/branding/logo", "theme": "system" }));
    let (status, saved) = world.send(Method::PUT, "/api/branding", Some(json!({ "theme": "dark" }))).await;
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

#[tokio::test(flavor = "multi_thread")]
async fn a_connection_saved_without_its_choice_learns_the_only_one_and_a_sign_in_with_none_fails() {
    let world = World::new().await;
    let record = store::ConnectionRecord {
        id: "c2".into(),
        provider: "picky".into(),
        label: "Picky".into(),
        account: Some("Picky".into()),
        capabilities: vec!["tracker".into()],
        settings: json!({}),
    };
    world.store.save_connection(&record).expect("connection");
    let grant = connectors::Grant::bearer("tok");
    world.connectors.keeper().store().save(&domain::connection::ConnectionId::from("c2"), &grant).await.expect("grant");
    let call = json!({ "action": "tracker.search", "args": { "query": "x" } });
    let found = world.send(Method::POST, "/api/connections/c2/call", Some(call)).await;
    assert_eq!(found.0, StatusCode::OK, "{}", found.1);
    assert_eq!(found.1[0]["title"], "only.test");
    let kept = world.store.connection("c2").expect("store").expect("record");
    assert_eq!(kept.settings["site"], "only.test");
    let started = world.send(Method::POST, "/api/connections/picky/start", Some(json!({ "fields": {} }))).await.1;
    let flow = started["flow"].clone();
    let mut last = Value::Null;
    for _ in 0..3 {
        last = world.send(Method::POST, "/api/connections/device/poll", Some(json!({ "flow": flow }))).await.1;
    }
    assert_eq!(last["state"], "failed", "{last}");
    assert!(last["message"].as_str().is_some_and(|text| text.contains("Picky")), "{last}");
    assert_eq!(world.store.connections().expect("store").len(), 2, "nothing was saved without a choice");
}

#[tokio::test(flavor = "multi_thread")]
async fn the_browser_comes_back_to_the_connections_screen_after_signing_in() {
    let world = World::new().await;
    let started = world.send(Method::POST, "/api/connections/web/start", Some(json!({ "fields": {} }))).await.1;
    assert_eq!(started["returns"], true);
    let flow = started["flow"].as_str().expect("flow").to_owned();
    let url = started["url"].as_str().expect("url");
    let state = url.split(['?', '&']).find_map(|pair| pair.strip_prefix("state=")).expect("state");
    let request = axum::http::Request::builder()
        .uri(format!("/oauth/callback?error=access_denied&state={state}"))
        .body(axum::body::Body::empty())
        .expect("request");
    let response = tower::ServiceExt::oneshot(world.router.clone(), request).await.expect("response");
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    let location = response.headers()["location"].to_str().expect("location").to_owned();
    assert_eq!(location, format!("/settings/connections?connect=web&flow={flow}"));
    let polled = world.send(Method::POST, "/api/connections/device/poll", Some(json!({ "flow": flow }))).await.1;
    assert_eq!(polled["state"], "failed");
}

#[tokio::test(flavor = "multi_thread")]
async fn an_api_token_connects_without_a_browser_and_is_kept_without_expiry() {
    let world = World::new().await;
    let listed = world.get("/api/connections").await;
    let web = listed["providers"].as_array().and_then(|all| all.iter().find(|p| p["key"] == "web")).cloned();
    let alternative = web.map(|web| web["alternatives"][0].clone()).unwrap_or_default();
    assert_eq!(alternative["signIn"], "token", "{alternative}");
    assert_eq!(alternative["fields"][0]["secret"], true);
    assert_eq!(alternative["fields"][0]["help"], "Gere o token.");
    let start = |token: &str| json!({ "fields": { "token": token }, "method": "api_token" });
    let empty = world.send(Method::POST, "/api/connections/web/start", Some(start("  "))).await;
    assert_eq!(empty.0, StatusCode::BAD_REQUEST, "{}", empty.1);
    let refused = world.send(Method::POST, "/api/connections/web/start", Some(start("recusado"))).await.1;
    let polled =
        world.send(Method::POST, "/api/connections/device/poll", Some(json!({ "flow": refused["flow"] }))).await.1;
    assert_eq!(polled["state"], "failed", "{polled}");
    let (status, started) = world.send(Method::POST, "/api/connections/web/start", Some(start(" zt-123 "))).await;
    assert_eq!(status, StatusCode::OK, "{started}");
    assert_eq!(started["kind"], "token");
    let done =
        world.send(Method::POST, "/api/connections/device/poll", Some(json!({ "flow": started["flow"] }))).await.1;
    assert_eq!(done["state"], "done", "{done}");
    let id = done["connection"]["id"].as_str().expect("id").to_owned();
    let kept = world.connectors.keeper().store().load(&domain::connection::ConnectionId::from(id.as_str())).await;
    let kept = kept.expect("store").expect("grant");
    assert_eq!(
        (kept.access_token.as_str(), kept.expires_at, kept.method.as_deref()),
        ("zt-123", None, Some("api_token"))
    );
}

fn folder(world: &World, name: &str) -> String {
    let path = world.dir.path().join(name);
    std::fs::create_dir_all(&path).expect("dir");
    std::fs::canonicalize(path).expect("path").to_string_lossy().into_owned()
}

async fn application(world: &World, repo: &str) -> (String, String) {
    let body = json!({
        "name": "Loja",
        "repositories": [{ "code": { "connection": "c1", "repo": repo }, "directory": folder(world, "web") }],
    });
    let (status, made) = world.send(Method::POST, "/api/applications", Some(body)).await;
    assert_eq!(status, StatusCode::OK, "{made}");
    let id = made["id"].as_str().expect("id").to_owned();
    (id, made["repositories"][0]["id"].as_str().expect("repository").to_owned())
}

fn kept(world: &World, item: &Value) -> Value {
    let id = domain::workflow::ItemId::from(item["id"].as_str().expect("id"));
    world.handle.item(&id).expect("store").expect("item").input
}

#[tokio::test(flavor = "multi_thread")]
async fn a_new_item_needs_no_issue_and_keeps_the_base_branch_chosen_per_repository() {
    let world = World::new().await;
    let (app, repository) = application(&world, "acme/web").await;
    let body = json!({
        "text": "Exportar relatórios", "sources": [], "model": "auto", "attachments": [], "application": app,
        "git": { repository.clone(): { "base": " release/2 ", "head": "" } },
    });
    let (status, item) = world.send(Method::POST, "/api/items", Some(body)).await;
    assert_eq!(status, StatusCode::OK, "{item}");
    assert_eq!(item["sources"], json!([]));
    assert_eq!(item["issue"], Value::Null);
    assert_eq!(item["workflow"], "default");
    assert_eq!(
        item["git"],
        json!([{ "repository": repository, "name": "acme/web", "base": "release/2", "head": null }])
    );
    let input = kept(&world, &item);
    assert_eq!(input["git"], json!({ repository.clone(): { "base": "release/2" } }));
    assert!(input["issue"].is_null());

    let stranger = json!({ "text": "x", "application": app, "git": { "nope": { "base": "main" } } });
    assert_eq!(world.send(Method::POST, "/api/items", Some(stranger)).await.0, StatusCode::BAD_REQUEST);
    let unknown = json!({ "text": "x", "workflow": "nenhum" });
    assert_eq!(world.send(Method::POST, "/api/items", Some(unknown)).await.0, StatusCode::BAD_REQUEST);
    let odd = json!({ "text": "x", "sources": [{ "kind": "ticket", "subject": "K-1" }] });
    assert_eq!(world.send(Method::POST, "/api/items", Some(odd)).await.0, StatusCode::BAD_REQUEST);
    let branch = json!({ "sources": [{ "kind": "branch", "subject": "feature/x" }], "application": app });
    let (status, item) = world.send(Method::POST, "/api/items", Some(branch)).await;
    assert_eq!(status, StatusCode::OK, "{item}");
    assert_eq!(item["title"], "feature/x");
    assert_eq!(kept(&world, &item)["git"], json!({ repository: { "head": "feature/x" } }));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_pull_request_origin_is_read_through_the_code_connection_and_finds_its_repository() {
    let world = World::new().await;
    let alone = json!({ "text": "", "sources": [{ "kind": "pull_request", "connection": "c1", "subject": "3" }] });
    let (status, item) = world.send(Method::POST, "/api/items", Some(alone.clone())).await;
    assert_eq!(status, StatusCode::OK, "{item}");
    assert_eq!(kept(&world, &item)["git"], json!({ "": { "base": "main", "head": "h" } }), "the only repository");
    application(&world, "acme/web").await;
    let (status, error) = world.send(Method::POST, "/api/items", Some(alone)).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "no application has the repository");
    assert!(error["error"]["message"].as_str().is_some_and(|text| text.contains("o/r")), "{error}");

    let (app, repository) = application(&world, "o/r").await;
    let body = json!({
        "text": "", "sources": [{ "kind": "pull_request", "connection": null, "subject": "3" }],
        "model": "auto", "attachments": [],
    });
    let (status, item) = world.send(Method::POST, "/api/items", Some(body)).await;
    assert_eq!(status, StatusCode::OK, "{item}");
    assert_eq!(item["title"], "PR");
    assert_eq!(item["workflow"], "pull-request", "the workflow that accepts pull requests");
    assert_eq!(item["sources"][0]["kind"], "pull_request");
    assert_eq!(item["sources"][0]["key"], "o/r#3");
    assert_eq!(item["sources"][0]["repository"], repository);
    assert_eq!(item["change"]["key"], "o/r#3");
    assert_eq!(item["git"][0]["base"], "main");
    assert_eq!(item["git"][0]["head"], "h");
    let input = kept(&world, &item);
    assert_eq!(input["application"]["id"], app, "the application of the repository");
    assert_eq!(input["git"], json!({ repository: { "base": "main", "head": "h" } }));
    assert!(input["issue"].is_null());
    let asked = world.provider.calls.lock().expect("calls").clone();
    assert!(asked.iter().any(|(action, args)| action == "code.pull_request" && args["pr"] == "3"));

    let chosen = json!({
        "text": "Revisar", "workflow": "default", "attachments": [],
        "sources": [{ "kind": "pull_request", "subject": "3" }, { "kind": "issue", "connection": "c1", "subject": "K-1" }],
    });
    let (status, item) = world.send(Method::POST, "/api/items", Some(chosen)).await;
    assert_eq!(status, StatusCode::OK, "{item}");
    assert_eq!(item["workflow"], "default");
    assert_eq!(item["title"], "Revisar");
    assert_eq!(item["issue"]["key"], "K-1");
    assert_eq!(kept(&world, &item)["issue"], "K-1");
}

async fn requested(world: &World, wanted: impl Fn(&domain::PromptRequest) -> bool) -> domain::PromptRequest {
    for _ in 0..200 {
        if let Some(found) = world.engine.requests().into_iter().rfind(|request| wanted(request)) {
            return found;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("the agent never got that request")
}

#[tokio::test(flavor = "multi_thread")]
async fn btw_opens_a_conversation_in_the_application_folder_that_goes_on_without_making_an_item() {
    let world = World::new().await;
    let (app, _) = application(&world, "acme/web").await;
    let body = json!({ "text": " o swagger está atualizado? ", "application": app, "model": "auto" });
    let (status, opened) = world.send(Method::POST, "/api/btw", Some(body)).await;
    assert_eq!(status, StatusCode::OK, "{opened}");
    assert_eq!(opened["session"], SESSION);
    let asked = requested(&world, |r| r.prompt.text == "o swagger está atualizado?").await;
    assert_eq!(asked.session.cwd.to_string_lossy(), folder(&world, "web"));
    assert_eq!(asked.policy.permission, domain::PermissionMode::Plan);
    assert!(asked.session.resume.is_none(), "a fresh session, not one of an item");
    for _ in 0..200 {
        if world.get(&format!("/api/sessions/{SESSION}")).await["status"] == "idle" {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let more = json!({ "text": "e o readme?", "model": null, "attachments": [] });
    let sent = world.send(Method::POST, &format!("/api/sessions/{SESSION}/messages"), Some(more)).await;
    assert_eq!(sent.0, StatusCode::NO_CONTENT, "{}", sent.1);
    let next = requested(&world, |r| r.prompt.text == "e o readme?").await;
    assert_eq!(next.session.resume.as_ref().map(|s| s.0.as_str()), Some(SESSION));
    let (_, items) = world.send(Method::GET, "/api/items", None).await;
    assert_eq!(items.as_array().map(Vec::len), Some(0), "{items}");
    let empty = json!({ "text": "  ", "application": app });
    assert_eq!(world.send(Method::POST, "/api/btw", Some(empty)).await.0, StatusCode::BAD_REQUEST);
    let nowhere = json!({ "text": "oi?" });
    assert_eq!(world.send(Method::POST, "/api/btw", Some(nowhere)).await.0, StatusCode::BAD_REQUEST);
}

#[tokio::test(flavor = "multi_thread")]
async fn the_browser_can_be_turned_off_and_back_on() {
    let world = World::new().await;
    assert_eq!(world.get("/api/browser").await["enabled"], json!(true));
    let (status, off) = world.send(Method::PUT, "/api/browser", Some(json!({ "enabled": false }))).await;
    assert_eq!((status, &off["enabled"]), (StatusCode::OK, &json!(false)));
    assert_eq!(world.get("/api/browser").await["enabled"], json!(false));
    let (status, _) = world.send(Method::PUT, "/api/browser", Some(json!({ "enabled": "no" }))).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (_, on) = world.send(Method::PUT, "/api/browser", Some(json!({ "enabled": true }))).await;
    assert_eq!(on["enabled"], json!(true));
}
