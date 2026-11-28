#![allow(clippy::expect_used, reason = "a failed setup step stops the test")]

mod support;

use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use domain::principal::{Principal, UserId};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use support::World;
use tower::ServiceExt;

fn signed(world: &World, login: &str, email: &str) -> String {
    let person = Principal { user: UserId(format!("u-{login}")), email: email.into(), name: String::new() };
    let person = world.store.ensure_user(&person, 0).expect("user");
    let token = format!("token-{login}");
    let hashed = platform::crypto::hex(&platform::crypto::sha256(token.as_bytes()));
    world.store.open_session(&hashed, &person.user, 0, i64::MAX).expect("session");
    format!("{}_session={token}", brand::COMMAND.replace('-', "_"))
}

async fn send(world: &World, cookie: &str, method: Method, path: &str, body: Option<Value>) -> (StatusCode, Value) {
    let request = Request::builder().method(method).uri(path).header("host", "127.0.0.1:7999").header("cookie", cookie);
    let request = match body {
        Some(body) => request.header("content-type", "application/json").body(Body::from(body.to_string())),
        None => request.body(Body::empty()),
    };
    let response = world.router.clone().oneshot(request.expect("request")).await.expect("response");
    let status = response.status();
    let bytes = response.into_body().collect().await.expect("body").to_bytes();
    (status, serde_json::from_slice(&bytes).unwrap_or(Value::Null))
}

async fn shared_world() -> World {
    server::force_auth_mode(server::AuthMode::Shared);
    World::new().await
}

fn member() -> String {
    brand::access::DEFAULT_ROLE.to_owned()
}

fn author() -> String {
    brand::roles::PAIRS
        .iter()
        .find(|(_, grants)| grants.contains("author") && !grants.contains("admin"))
        .map(|(name, _)| (*name).to_owned())
        .expect("a role that edits content")
}

#[tokio::test(flavor = "multi_thread")]
async fn someone_in_the_use_group_runs_workflows_but_does_not_edit_content() {
    let world = shared_world().await;
    let ana = signed(&world, "ana", "ana@example.com");
    world.store.set_roles(&UserId("u-ana".into()), "sso", &[member()]).expect("roles");
    let (_, me) = send(&world, &ana, Method::GET, "/api/auth/me", None).await;
    assert_eq!(me["roles"], json!([member()]), "{me}");
    assert_eq!(me["grants"], json!(["use"]), "{me}");
    assert_eq!(me["admin"], false);
    let made = json!({ "text": "Corrigir o login", "issue": null, "model": "auto", "attachments": [] });
    let (status, item) = send(&world, &ana, Method::POST, "/api/items", Some(made)).await;
    assert_eq!(status, StatusCode::OK, "{item}");
    let saved = json!({ "kind": "agent", "name": "dev", "text": "x" });
    let (status, body) = send(&world, &ana, Method::PUT, "/api/library/item", Some(saved)).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(body["error"]["code"], "role_denied");
    let (status, _) = send(&world, &ana, Method::GET, "/api/library", None).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    world.store.set_roles(&UserId("u-ana".into()), "assigned", &[author()]).expect("author");
    let (status, body) = send(&world, &ana, Method::GET, "/api/library", None).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let (status, _) = send(&world, &ana, Method::PUT, "/api/permissions", Some(json!({}))).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test(flavor = "multi_thread")]
async fn an_administrator_gives_a_declared_role_to_a_person() {
    let folder = tempfile::tempdir().expect("folder");
    let file = folder.path().join("setup.token");
    std::fs::write(&file, "a-setup-token-long-enough\n").expect("token");
    server::force_setup_file(file.to_str().expect("path"));
    let world = shared_world().await;
    let claim = json!({ "token": "a-setup-token-long-enough", "login": "boss", "password": "a long secret" });
    let request = Request::builder()
        .method(Method::POST)
        .uri("/api/auth/setup")
        .header("host", "127.0.0.1:7999")
        .header("content-type", "application/json")
        .body(Body::from(claim.to_string()))
        .expect("request");
    assert_eq!(world.router.clone().oneshot(request).await.expect("response").status(), StatusCode::OK);
    let boss = signed(&world, "boss", "boss");
    let bia = signed(&world, "bia", "bia@example.com");
    let (_, roles) = send(&world, &boss, Method::GET, "/api/admin/roles", None).await;
    assert_eq!(roles["defaultRole"], member(), "{roles}");
    let wrong = json!({ "roles": ["ghost"] });
    let (status, _) = send(&world, &boss, Method::PUT, "/api/admin/people/u-bia/roles", Some(wrong)).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let given = json!({ "roles": [author()] });
    let (status, _) = send(&world, &bia, Method::PUT, "/api/admin/people/u-bia/roles", Some(given.clone())).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, body) = send(&world, &boss, Method::PUT, "/api/admin/people/u-bia/roles", Some(given)).await;
    assert!(status.is_success(), "{body}");
    let (_, people) = send(&world, &boss, Method::GET, "/api/admin/people", None).await;
    let bia_row = people.as_array().and_then(|all| all.iter().find(|row| row["id"] == "u-bia").cloned());
    let bia_row = bia_row.expect("bia is listed");
    assert_eq!(bia_row["assigned"], json!([author()]), "{bia_row}");
    assert_eq!(bia_row["admin"], false);
    let boss_row = people.as_array().and_then(|all| all.iter().find(|row| row["email"] == "boss").cloned());
    assert_eq!(boss_row.expect("boss")["admin"], true);
    let (status, _) = send(&world, &bia, Method::GET, "/api/library", None).await;
    assert_eq!(status, StatusCode::OK);
}
