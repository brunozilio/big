#![allow(clippy::expect_used, reason = "a failed setup step stops the test")]

mod support;

use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use domain::audit::AuditEvent;
use domain::principal::{Principal, UserId};
use serde_json::Value;
use support::World;
use tower::ServiceExt;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

fn draft(at: i64, agent: &str, person: &str, action: &str) -> AuditEvent {
    AuditEvent {
        actor: Some(agent.into()),
        subject: Some(person.into()),
        outcome: Some("success".into()),
        detail: serde_json::json!({}),
        ..AuditEvent::new(at, action)
    }
}

async fn found(world: &World, path: &str) -> Value {
    for _ in 0..200 {
        let listed = world.get(path).await;
        if listed["records"].as_array().is_some_and(|records| !records.is_empty()) {
            return listed;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!("nothing reached {path}")
}

#[tokio::test(flavor = "multi_thread")]
async fn an_audit_event_reaches_the_trail_and_is_filtered_exported_and_verified() {
    tracing_subscriber::registry().with(server::trail::layer()).try_init().expect("subscriber");
    let world = World::new().await;
    server::trail::attach(Arc::clone(&world.store), Arc::default());
    tracing::warn!(target: "audit", agent = "dev", user = "u-ana", layer = "floor", outcome = "denied", "policy.blocked");
    let listed = found(&world, "/api/admin/audit?agent=dev").await;
    let record = &listed["records"][0];
    assert_eq!(record["action"], "policy.blocked");
    assert_eq!(record["subject"], "u-ana");
    assert_eq!(record["policy"], "floor");
    assert_eq!(record["outcome"], "denied");
    assert_eq!(record["schemaVersion"], domain::audit::SCHEMA_VERSION);
    assert_eq!(record["layer"], "floor");
    assert_eq!(record["failureStage"], "authorization");
    world.store.append_audit(&[draft(1_000, "review", "u-bia", "tool.call")]).expect("old");
    let old = world.get("/api/admin/audit?agent=review&from=500&to=1500").await;
    assert_eq!(old["records"].as_array().map(Vec::len), Some(1), "{old}");
    let none = world.get("/api/admin/audit?agent=review&from=2000").await;
    assert_eq!(none["records"].as_array().map(Vec::len), Some(0), "{none}");
    let (status, exported) = world.send(Method::GET, "/api/admin/audit/export?person=u-bia", None).await;
    assert_eq!(status, StatusCode::OK);
    let lines: Vec<Value> = exported
        .as_str()
        .map(|text| text.lines().map(|line| serde_json::from_str(line).expect("a json line")).collect())
        .unwrap_or_else(|| vec![exported.clone()]);
    assert_eq!(lines.len(), 1, "{exported}");
    assert_eq!(lines[0]["actor"], "review");
    let verified = world.get("/api/admin/audit/verify").await;
    assert_eq!(verified["broken"], Value::Null, "{verified}");
    assert_eq!(verified["checked"], 2);
    let facets = world.get("/api/admin/audit/facets").await;
    assert_eq!(facets["actors"], serde_json::json!(["dev", "review"]));
}

#[tokio::test(flavor = "multi_thread")]
async fn the_trail_pages_from_the_newest() {
    let world = World::new().await;
    let drafts: Vec<AuditEvent> = (1..=5).map(|at| draft(at, "dev", "u-ana", "tool.call")).collect();
    world.store.append_audit(&drafts).expect("append");
    let first = world.get("/api/admin/audit?limit=2").await;
    let seqs = |page: &Value| {
        page["records"].as_array().expect("records").iter().map(|r| r["seq"].clone()).collect::<Vec<_>>()
    };
    assert_eq!(seqs(&first), [5, 4]);
    assert_eq!(first["next"], 4);
    let second = world.get("/api/admin/audit?limit=2&before=4").await;
    assert_eq!(seqs(&second), [3, 2]);
    let last = world.get("/api/admin/audit?limit=2&before=2").await;
    assert_eq!(seqs(&last), [1]);
    assert_eq!(last["next"], Value::Null);
}

#[tokio::test(flavor = "multi_thread")]
async fn only_an_administrator_reads_or_exports_the_trail_when_shared() {
    server::force_auth_mode(server::AuthMode::Shared);
    let world = World::new().await;
    let person = Principal { user: UserId("ana".into()), email: "ana@example.com".into(), name: String::new() };
    world.store.ensure_user(&person, 0).expect("user");
    let hashed = platform::crypto::hex(&platform::crypto::sha256(b"token-ana"));
    world.store.open_session(&hashed, &person.user, 0, i64::MAX).expect("session");
    let cookie = format!("{}_session=token-ana", brand::COMMAND.replace('-', "_"));
    for path in ["/api/admin/audit", "/api/admin/audit/export", "/api/admin/audit/verify"] {
        let request = Request::builder()
            .method(Method::GET)
            .uri(path)
            .header("host", "127.0.0.1:7999")
            .header("cookie", &cookie)
            .body(Body::empty())
            .expect("request");
        let response = world.router.clone().oneshot(request).await.expect("response");
        assert_eq!(response.status(), StatusCode::FORBIDDEN, "{path}");
    }
}
