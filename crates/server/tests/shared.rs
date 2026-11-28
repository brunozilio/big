#![allow(clippy::expect_used, reason = "a failed setup step stops the test")]

mod support;

use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use domain::principal::{Principal, UserId};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use support::World;
use tower::ServiceExt;

fn signed(world: &World, user: &str) -> String {
    let person = Principal { user: UserId(user.into()), email: format!("{user}@example.com"), name: String::new() };
    world.store.ensure_user(&person, 0).expect("user");
    let token = format!("token-{user}");
    let hashed = platform::crypto::hex(&platform::crypto::sha256(token.as_bytes()));
    world.store.open_session(&hashed, &person.user, 0, i64::MAX).expect("session");
    format!("{}_session={token}", brand::COMMAND.replace('-', "_"))
}

async fn send(
    world: &World,
    cookie: Option<&str>,
    method: Method,
    path: &str,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let mut request = Request::builder().method(method).uri(path).header("host", "127.0.0.1:7999");
    if let Some(cookie) = cookie {
        request = request.header("cookie", cookie);
    }
    let body = match body {
        Some(body) => {
            request = request.header("content-type", "application/json");
            Body::from(body.to_string())
        }
        None => Body::empty(),
    };
    let response = world.router.clone().oneshot(request.body(body).expect("request")).await.expect("response");
    let status = response.status();
    let bytes = response.into_body().collect().await.expect("body").to_bytes();
    (status, serde_json::from_slice(&bytes).unwrap_or(Value::Null))
}

async fn shared_world() -> World {
    server::force_auth_mode(server::AuthMode::Shared);
    World::new().await
}

#[tokio::test(flavor = "multi_thread")]
async fn without_a_session_only_the_sign_in_is_open() {
    let world = shared_world().await;
    let (status, body) = send(&world, None, Method::GET, "/api/items", None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "{body}");
    assert_eq!(body["error"]["code"], "signed_out");
    let (status, _) = send(&world, None, Method::GET, "/api/auth/methods", None).await;
    assert_eq!(status, StatusCode::OK);
    let (status, _) = send(&world, None, Method::GET, "/api/branding", None).await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test(flavor = "multi_thread")]
async fn each_person_sees_only_their_own_applications() {
    let world = shared_world().await;
    let (ana, bia) = (signed(&world, "ana"), signed(&world, "bia"));
    let folder = world.dir.path().join("loja");
    std::fs::create_dir_all(&folder).expect("dir");
    let folder = std::fs::canonicalize(folder).expect("path").to_string_lossy().into_owned();
    let made = json!({ "name": "Loja", "repositories": [{ "directory": folder }] });
    let (status, created) = send(&world, Some(&ana), Method::POST, "/api/applications", Some(made)).await;
    assert_eq!(status, StatusCode::OK, "{created}");
    let id = created["id"].as_str().expect("id").to_owned();
    let (_, mine) = send(&world, Some(&ana), Method::GET, "/api/applications", None).await;
    assert_eq!(mine.as_array().map(Vec::len), Some(1), "{mine}");
    let (_, theirs) = send(&world, Some(&bia), Method::GET, "/api/applications", None).await;
    assert_eq!(theirs.as_array().map(Vec::len), Some(0), "{theirs}");
    let path = format!("/api/applications/{id}");
    let (status, _) = send(&world, Some(&bia), Method::DELETE, &path, None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test(flavor = "multi_thread")]
async fn connections_of_the_local_install_are_not_shown_to_signed_in_people() {
    let world = shared_world().await;
    let ana = signed(&world, "ana");
    let (status, listed) = send(&world, Some(&ana), Method::GET, "/api/connections", None).await;
    assert_eq!(status, StatusCode::OK, "{listed}");
    let ids: Vec<&str> =
        listed["connections"].as_array().expect("list").iter().filter_map(|c| c["id"].as_str()).collect();
    assert!(!ids.contains(&"c1"), "{listed}");
    let (status, _) = send(&world, Some(&ana), Method::DELETE, "/api/connections/c1", None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test(flavor = "multi_thread")]
async fn settings_for_everyone_need_an_administrator() {
    let world = shared_world().await;
    let ana = signed(&world, "ana");
    let (status, body) = send(&world, Some(&ana), Method::PUT, "/api/permissions", Some(json!({}))).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    let (status, _) = send(&world, Some(&ana), Method::GET, "/api/fs/dirs", None).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_request_from_another_site_is_refused() {
    let world = shared_world().await;
    let ana = signed(&world, "ana");
    let request = Request::builder()
        .method(Method::POST)
        .uri("/api/applications")
        .header("cookie", ana)
        .header("origin", "https://evil.example")
        .header("content-type", "application/json")
        .body(Body::from("{}"))
        .expect("request");
    let response = world.router.clone().oneshot(request).await.expect("response");
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test(flavor = "multi_thread")]
async fn the_server_clones_only_https_repositories() {
    let world = shared_world().await;
    let ana = signed(&world, "ana");
    let made = json!({ "name": "Loja", "repositories": [{ "directory": "", "remote": "file:///etc" }] });
    let (status, body) = send(&world, Some(&ana), Method::POST, "/api/applications", Some(made)).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    let (status, _) = send(&world, None, Method::GET, "/api/git/credentials/nope?url=https://x", None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test(flavor = "multi_thread")]
async fn only_an_administrator_lists_people_and_signs_them_out() {
    let world = shared_world().await;
    let ana = signed(&world, "ana");
    let (status, _) = send(&world, Some(&ana), Method::GET, "/api/admin/people", None).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, _) = send(&world, Some(&ana), Method::PUT, "/api/branding", Some(json!({ "language": "es" }))).await;
    assert_eq!(status, StatusCode::OK);
    let (_, mine) = send(&world, Some(&ana), Method::GET, "/api/branding", None).await;
    assert_eq!(mine["language"], "es");
    let (_, others) = send(&world, None, Method::GET, "/api/branding", None).await;
    assert_ne!(others["language"], "es");
    let (status, _) = send(&world, Some(&ana), Method::PUT, "/api/onboarding", Some(json!({ "skipped": true }))).await;
    assert_eq!(status, StatusCode::OK);
}

fn with_password(world: &World, login: &str, password: &str) -> UserId {
    let person = Principal { user: UserId(format!("u-{login}")), email: login.into(), name: String::new() };
    world.store.ensure_user(&person, 0).expect("user");
    let hash = platform::password::hash_with(password, 1_000).expect("hash");
    world.store.set_password(&person.user, &hash, 0).expect("password");
    person.user
}

#[tokio::test(flavor = "multi_thread")]
async fn a_person_signs_in_with_user_and_password_and_too_many_misses_lock_the_user() {
    let world = shared_world().await;
    with_password(&world, "bia", "a long secret");
    let sign = |login: &str, password: &str| json!({ "login": login, "password": password });
    let (status, body) =
        send(&world, None, Method::POST, "/api/auth/password", Some(sign(" BIA ", "a long secret"))).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["email"], "bia");
    let (status, _) =
        send(&world, None, Method::POST, "/api/auth/password", Some(sign("nobody", "a long secret"))).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    for _ in 0..5 {
        let (status, _) = send(&world, None, Method::POST, "/api/auth/password", Some(sign("bia", "wrong one"))).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }
    let (status, body) =
        send(&world, None, Method::POST, "/api/auth/password", Some(sign("bia", "a long secret"))).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_person_changes_their_own_password_only_knowing_the_current_one() {
    let world = shared_world().await;
    let user = with_password(&world, "caio", "first secret");
    let token = "token-caio";
    let hashed = platform::crypto::hex(&platform::crypto::sha256(token.as_bytes()));
    world.store.open_session(&hashed, &user, 0, i64::MAX).expect("session");
    let cookie = format!("{}_session={token}", brand::COMMAND.replace('-', "_"));
    let change = |current: &str, password: &str| json!({ "current": current, "password": password });
    let (status, _) =
        send(&world, Some(&cookie), Method::PUT, "/api/account/password", Some(change("nope", "second secret"))).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (status, _) =
        send(&world, Some(&cookie), Method::PUT, "/api/account/password", Some(change("first secret", "short"))).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (status, body) = send(
        &world,
        Some(&cookie),
        Method::PUT,
        "/api/account/password",
        Some(change("first secret", "second secret")),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT, "{body}");
    let stored = world.store.password_of(&user).expect("read").expect("there");
    assert!(platform::password::matches(&stored, "second secret"));
}

#[tokio::test(flavor = "multi_thread")]
async fn only_an_administrator_adds_people_or_sets_their_password() {
    let world = shared_world().await;
    let ana = signed(&world, "ana");
    let added = json!({ "login": "dora", "name": "Dora", "password": "a long secret" });
    let (status, _) = send(&world, Some(&ana), Method::POST, "/api/admin/people", Some(added)).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let reset = json!({ "password": "a long secret" });
    let (status, _) = send(&world, Some(&ana), Method::PUT, "/api/admin/people/u-ana/password", Some(reset)).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test(flavor = "multi_thread")]
async fn the_first_admin_is_claimed_once_with_the_setup_token() {
    let folder = tempfile::tempdir().expect("folder");
    let file = folder.path().join("setup.token");
    std::fs::write(&file, "a-setup-token-long-enough\n").expect("token");
    server::force_setup_file(file.to_str().expect("path"));
    let world = shared_world().await;
    let (_, methods) = send(&world, None, Method::GET, "/api/auth/methods", None).await;
    let admin = server::configured_admins().into_iter().next().unwrap_or_else(|| "boss".to_owned());
    assert_eq!(methods["setup"], true, "{methods}");
    let claim = |token: &str, login: &str| json!({ "token": token, "login": login, "password": "a long secret" });
    let (status, _) = send(&world, None, Method::POST, "/api/auth/setup", Some(claim("wrong", &admin))).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (status, body) =
        send(&world, None, Method::POST, "/api/auth/setup", Some(claim("a-setup-token-long-enough", &admin))).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(server::admins_now().contains(&admin));
    let (_, methods) = send(&world, None, Method::GET, "/api/auth/methods", None).await;
    assert_eq!(methods["setup"], false, "{methods}");
    let (status, _) =
        send(&world, None, Method::POST, "/api/auth/setup", Some(claim("a-setup-token-long-enough", "someone-else")))
            .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_sign_in_that_comes_back_to_this_computer_is_relayed_to_the_server() {
    let world = shared_world().await;
    let ana = signed(&world, "ana");
    let (status, started) =
        send(&world, Some(&ana), Method::POST, "/api/connections/web/start", Some(json!({ "fields": {} }))).await;
    assert_eq!(status, StatusCode::OK, "{started}");
    let from = started["relay"]["from"].as_str().unwrap_or_default();
    let to = started["relay"]["to"].as_str().unwrap_or_default();
    assert!(from.starts_with("http://127.0.0.1:") && !from.contains('?'), "{started}");
    assert!(to.ends_with("/oauth/callback"), "{started}");
    assert_eq!(started["paste"], true);
}

async fn as_browser(world: &World, token: &str, method: Method, path: &str, body: &str) -> axum::response::Response {
    let request = Request::builder()
        .method(method)
        .uri(path)
        .header("host", "127.0.0.1:7999")
        .header("origin", "chrome-extension://abc")
        .header("authorization", format!("Bearer {token}"))
        .body(Body::from(body.to_owned()))
        .expect("request");
    world.router.clone().oneshot(request).await.expect("response")
}

#[tokio::test(flavor = "multi_thread")]
async fn a_cloud_browser_pairs_with_a_one_time_code_and_belongs_to_whoever_paired_it() {
    let world = shared_world().await;
    let (ana, bia) = (signed(&world, "ana"), signed(&world, "bia"));
    let (status, pairing) = send(&world, Some(&ana), Method::POST, "/api/browser/pairing", None).await;
    assert_eq!(status, StatusCode::OK, "{pairing}");
    let code = pairing["code"].as_str().unwrap_or_default().to_owned();
    let claim = json!({ "code": code, "deviceId": "chrome-1", "name": "Ana's laptop" });
    let (status, claimed) = send(&world, None, Method::POST, "/api/browser/claim", Some(claim.clone())).await;
    assert_eq!(status, StatusCode::OK, "{claimed}");
    let token = claimed["token"].as_str().unwrap_or_default().to_owned();
    let (status, _) = send(&world, None, Method::POST, "/api/browser/claim", Some(claim)).await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    let refused = as_browser(&world, "wrong", Method::GET, "/api/browser/stream?build=x", "").await;
    assert_eq!(refused.status(), StatusCode::UNAUTHORIZED);
    let streaming = as_browser(&world, &token, Method::GET, "/api/browser/stream?build=x&platform=mac", "").await;
    assert_eq!(streaming.status(), StatusCode::OK);
    let mut body = streaming.into_body();
    let first = tokio::time::timeout(std::time::Duration::from_secs(5), body.frame()).await.expect("a first event");
    let first = first.and_then(Result::ok).and_then(|frame| frame.into_data().ok()).unwrap_or_default();
    assert!(String::from_utf8_lossy(&first).starts_with("data: "));

    let (_, mine) = send(&world, Some(&ana), Method::GET, "/api/browser", None).await;
    assert_eq!(mine["cloud"], true);
    assert_eq!(mine["devices"][0]["device"], "chrome-1");
    assert_eq!(mine["devices"][0]["connected"], true);
    assert_eq!(mine["browsers"].as_array().map(Vec::len), Some(1));
    let (_, theirs) = send(&world, Some(&bia), Method::GET, "/api/browser", None).await;
    assert_eq!(theirs["devices"], json!([]));
    assert_eq!(theirs["browsers"], json!([]));

    let answered = as_browser(&world, &token, Method::POST, "/api/browser/reply", r#"{"type":"pong"}"#).await;
    assert_eq!(answered.status(), StatusCode::NO_CONTENT);
    let (status, _) = send(&world, Some(&bia), Method::DELETE, "/api/browser/devices/chrome-1", None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = send(&world, Some(&ana), Method::DELETE, "/api/browser/devices/chrome-1", None).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let gone = as_browser(&world, &token, Method::GET, "/api/browser/stream?build=x", "").await;
    assert_eq!(gone.status(), StatusCode::UNAUTHORIZED);
    drop(body);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_signed_in_person_downloads_the_extension_package() {
    let world = shared_world().await;
    let ana = signed(&world, "ana");
    let request = Request::builder()
        .uri("/api/browser/extension.zip")
        .header("host", "127.0.0.1:7999")
        .header("cookie", ana)
        .body(Body::empty())
        .expect("request");
    let response = world.router.clone().oneshot(request).await.expect("response");
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()["content-type"], "application/zip");
    let bytes = response.into_body().collect().await.expect("body").to_bytes();
    assert!(bytes.starts_with(b"PK"));
}

#[tokio::test(flavor = "multi_thread")]
async fn the_ai_sign_in_return_needs_a_session_and_an_open_sign_in() {
    let world = shared_world().await;
    let (status, _) = send(&world, None, Method::GET, "/oauth/loopback/1455/auth/callback?code=x", None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let ana = signed(&world, "ana");
    let (status, _) = send(&world, Some(&ana), Method::GET, "/oauth/loopback/1455/auth/callback?code=x", None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

async fn paired_chrome(world: &World, cookie: &str, device: &str) -> String {
    let (_, pairing) = send(world, Some(cookie), Method::POST, "/api/browser/pairing", None).await;
    let claim = json!({ "code": pairing["code"], "deviceId": device, "name": device });
    let (_, claimed) = send(world, None, Method::POST, "/api/browser/claim", Some(claim)).await;
    claimed["token"].as_str().expect("token").to_owned()
}

async fn next_event(body: &mut Body) -> Value {
    let frame = tokio::time::timeout(std::time::Duration::from_secs(5), body.frame()).await.expect("an event");
    let data = frame.and_then(Result::ok).and_then(|frame| frame.into_data().ok()).unwrap_or_default();
    let text = String::from_utf8_lossy(&data);
    serde_json::from_str(text.trim().trim_start_matches("data: ")).unwrap_or(Value::Null)
}

async fn relayed(world: &World, token: &str, flow: &str, origin: &str, method: &str, body: &str) -> Value {
    let url = format!("http://localhost:{}/api/token", support::handoff_port());
    let sent = json!({ "flow": flow, "origin": origin, "request": { "method": method, "url": url, "body": body } });
    let answered = as_browser(world, token, Method::POST, "/api/browser/loopback", &sent.to_string()).await;
    assert_eq!(answered.status(), StatusCode::OK);
    let bytes = answered.into_body().collect().await.expect("body").to_bytes();
    serde_json::from_slice(&bytes).unwrap_or(Value::Null)
}

#[tokio::test(flavor = "multi_thread")]
async fn on_a_shared_server_the_paired_chrome_opens_the_page_and_relays_what_it_sends_to_the_local_port() {
    let world = shared_world().await;
    let (ana, bia) = (signed(&world, "ana"), signed(&world, "bia"));
    let start = json!({ "fields": {} });
    let (status, _) = send(&world, Some(&ana), Method::POST, "/api/connections/handoff/start", Some(start.clone())).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "no Chrome is paired yet");
    let token = paired_chrome(&world, &ana, "chrome-ana").await;
    let other = paired_chrome(&world, &bia, "chrome-bia").await;
    let streaming = as_browser(&world, &token, Method::GET, "/api/browser/stream?build=x&platform=mac", "").await;
    let mut events = streaming.into_body();
    let _ = next_event(&mut events).await;
    let (status, started) = send(&world, Some(&ana), Method::POST, "/api/connections/handoff/start", Some(start)).await;
    assert_eq!(status, StatusCode::OK, "{started}");
    assert_eq!((started["opened"].clone(), started["returns"].clone()), (json!(true), json!(false)), "{started}");
    let flow = started["flow"].as_str().expect("flow").to_owned();
    let mut watched = next_event(&mut events).await;
    while watched["type"] == "ping" {
        watched = next_event(&mut events).await;
    }
    assert_eq!((watched["type"].clone(), watched["flow"].clone()), (json!("watch"), json!(flow)), "{watched}");
    assert_eq!((watched["url"].clone(), watched["listen"].clone()), (started["url"].clone(), json!(support::handoff_port())));
    let origin = support::HANDOFF_ORIGIN;
    let stranger = relayed(&world, &token, &flow, "https://evil.test", "GET", "").await;
    assert_eq!(stranger["status"], 403, "{stranger}");
    let theirs = relayed(&world, &other, &flow, origin, "POST", r#"{"token":"stolen"}"#).await;
    assert_eq!(theirs["status"], 403, "{theirs}");
    let status = relayed(&world, &token, &flow, origin, "GET", "").await;
    assert_eq!(status, json!({ "status": 200, "body": { "needsToken": true }, "done": false }));
    let given = relayed(&world, &token, &flow, origin, "POST", r#"{"token":"squ_cloud"}"#).await;
    assert_eq!(given, json!({ "status": 200, "body": "OK", "done": true }));
    let poll = json!({ "flow": flow });
    let (_, done) = send(&world, Some(&ana), Method::POST, "/api/connections/device/poll", Some(poll)).await;
    assert_eq!((done["state"].clone(), done["connection"]["label"].clone()), (json!("done"), json!("handoff:squ_cloud")), "{done}");
    drop(events);
}

#[tokio::test(flavor = "multi_thread")]
async fn the_browser_logic_is_served_to_the_extension_without_a_session() {
    let world = shared_world().await;
    let request = Request::builder()
        .uri("/api/browser/logic.js")
        .header("host", "127.0.0.1:7999")
        .header("origin", "chrome-extension://abc")
        .body(Body::empty())
        .expect("request");
    let response = world.router.clone().oneshot(request).await.expect("response");
    assert_eq!(response.status(), StatusCode::OK);
    assert!(response.headers()["content-type"].to_str().unwrap_or_default().starts_with("text/javascript"));
    let bytes = response.into_body().collect().await.expect("body").to_bytes();
    assert!(String::from_utf8_lossy(&bytes).contains("globalThis.__logic"));
}
#[tokio::test(flavor = "multi_thread")]
async fn only_the_owner_kills_an_item_and_killing_an_agent_needs_an_administrator() {
    let world = shared_world().await;
    let (ana, bia) = (signed(&world, "ana"), signed(&world, "bia"));
    let made = json!({ "text": "Corrigir o login", "issue": null, "model": "auto", "attachments": [] });
    let (status, item) = send(&world, Some(&ana), Method::POST, "/api/items", Some(made)).await;
    assert_eq!(status, StatusCode::OK, "{item}");
    let path = format!("/api/items/{}/kill", item["id"].as_str().expect("id"));
    let (status, _) = send(&world, Some(&bia), Method::POST, &path, Some(json!({}))).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "someone else's item stays hidden");
    let (_, still) = send(&world, Some(&ana), Method::GET, "/api/items", None).await;
    let open = still.as_array().and_then(|items| items.iter().find(|found| found["id"] == item["id"]).cloned());
    assert!(open.is_some_and(|found| found["doneAt"].is_null()), "{still}");
    let (status, body) = send(&world, Some(&bia), Method::POST, "/api/agents/worker/kill", Some(json!({}))).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(body["error"]["code"], "admin_only");
    let (status, _) = send(&world, Some(&ana), Method::POST, &path, Some(json!({}))).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
}

fn titles(found: &Value) -> Vec<String> {
    let listed = found.as_array().map(Vec::as_slice).unwrap_or_default();
    listed.iter().filter_map(|entry| entry["item"]["title"].as_str().map(str::to_owned)).collect()
}

#[tokio::test(flavor = "multi_thread")]
async fn each_person_sees_only_what_waits_for_them() {
    let world = shared_world().await;
    let (ana, bia) = (signed(&world, "ana"), signed(&world, "bia"));
    for (cookie, text) in [(&ana, "Corrigir o login"), (&bia, "Trocar o logo")] {
        let made = json!({ "text": text, "issue": null, "model": "auto", "attachments": [] });
        let (status, item) = send(&world, Some(cookie), Method::POST, "/api/items", Some(made)).await;
        assert_eq!(status, StatusCode::OK, "{item}");
    }
    let mut seen = (Vec::new(), Vec::new());
    for _ in 0..400 {
        seen = (
            titles(&send(&world, Some(&ana), Method::GET, "/api/inbox", None).await.1),
            titles(&send(&world, Some(&bia), Method::GET, "/api/inbox", None).await.1),
        );
        if !seen.0.is_empty() && !seen.1.is_empty() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }
    assert_eq!(seen, (vec!["Corrigir o login".to_owned()], vec!["Trocar o logo".to_owned()]));
    let (status, _) = send(&world, Some(&bia), Method::POST, "/api/inbox/respond", Some(json!({ "ask": "nope" }))).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}
