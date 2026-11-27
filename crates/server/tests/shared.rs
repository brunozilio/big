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
