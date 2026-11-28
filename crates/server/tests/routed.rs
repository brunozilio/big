#![allow(clippy::expect_used, reason = "a failed setup step stops the test")]

mod support;

use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use domain::principal::{Principal, UserId};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use support::World;
use tower::ServiceExt;

const PURCHASE: &str = r#"
import { workflow, role, admins } from "sdk";
import { approve, handoff } from "sdk/steps";

export default workflow({
  name: "compra",
  steps: [
    approve({ text: (item) => `Aprovar ${item.title}`, assignTo: role("controller") }),
    handoff({ text: "Liberar o pagamento", assignTo: admins() }),
  ],
});
"#;

fn signed(world: &World, user: &str) -> String {
    let person = Principal { user: UserId(user.into()), email: format!("{user}@example.com"), name: user.into() };
    world.store.ensure_user(&person, 0).expect("user");
    let token = format!("token-{user}");
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

async fn inbox(world: &World, cookie: &str) -> Vec<Value> {
    send(world, cookie, Method::GET, "/api/inbox", None).await.1.as_array().cloned().unwrap_or_default()
}

async fn inbox_until(world: &World, cookie: &str, wanted: usize, step: &str) -> Vec<Value> {
    for _ in 0..400 {
        let found: Vec<Value> =
            inbox(world, cookie).await.into_iter().filter(|entry| entry["item"]["step"] == step).collect();
        if found.len() == wanted {
            return found;
        }
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }
    panic!("never {wanted} at {step}: {:?}", inbox(world, cookie).await)
}

async fn bought(world: &World, cookie: &str, title: &str) -> String {
    let made = json!({ "text": title, "workflow": "compra", "issue": null, "model": "auto", "attachments": [] });
    let (status, item) = send(world, cookie, Method::POST, "/api/items", Some(made)).await;
    assert_eq!(status, StatusCode::OK, "{item}");
    item["id"].as_str().expect("id").to_owned()
}

fn give(world: &World, user: &str, role: &str) {
    let roles = [role.to_owned(), brand::access::DEFAULT_ROLE.to_owned()];
    world.store.set_roles(&UserId(user.into()), "assigned", &roles).expect("role");
}

#[tokio::test(flavor = "multi_thread")]
async fn an_approval_for_a_role_reaches_only_its_members_is_claimed_once_and_three_are_approved_together() {
    server::force_auth_mode(server::AuthMode::Shared);
    let world = World::with(&[("workflows/compra.ts", PURCHASE)], Vec::new()).await;
    let (ana, bia, caio, root) =
        (signed(&world, "ana"), signed(&world, "bia"), signed(&world, "caio"), signed(&world, "root"));
    give(&world, "bia", "controller");
    give(&world, "caio", "controller");
    give(&world, "root", brand::access::ADMIN_ROLE);
    let mut items = Vec::new();
    for title in ["Notebook", "Monitor", "Cadeira", "Mesa"] {
        items.push(bought(&world, &ana, title).await);
    }
    let seen = inbox_until(&world, &bia, 4, "approve").await;
    assert_eq!(seen[0]["assignedTo"], json!({ "kind": "role", "name": "controller" }), "{seen:?}");
    assert!(inbox(&world, &ana).await.is_empty(), "the owner is not a controller");
    assert!(inbox(&world, &root).await.is_empty(), "nor is the admin");
    let id = |item: &str| format!("{item}:approve");
    let (status, held) =
        send(&world, &bia, Method::POST, "/api/inbox/claim", Some(json!({ "id": id(&items[0]) }))).await;
    assert_eq!(status, StatusCode::OK, "{held}");
    let (status, refused) =
        send(&world, &caio, Method::POST, "/api/inbox/claim", Some(json!({ "id": id(&items[0]) }))).await;
    assert_eq!(status, StatusCode::CONFLICT, "{refused}");
    assert_eq!(refused["error"]["code"], "claimed");
    let theirs = inbox(&world, &caio).await;
    let taken = theirs.iter().find(|entry| entry["id"] == id(&items[0]).as_str()).expect("still listed");
    assert_eq!(taken["claim"], json!({ "by": "bia", "at": taken["claim"]["at"], "mine": false }), "{taken}");
    let approve = json!({ "action": "approve" });
    let answer = json!({ "ask": id(&items[0]), "response": approve });
    assert_eq!(send(&world, &caio, Method::POST, "/api/inbox/respond", Some(answer)).await.0, StatusCode::CONFLICT);
    let path = format!("/api/items/{}/steps/approve/input", items[3]);
    let (status, body) = send(&world, &ana, Method::POST, &path, Some(json!({ "action": "approve" }))).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "the owner cannot approve for the controller: {body}");
    let three: Vec<String> = items[..3].iter().map(|item| id(item)).collect();
    let batch = json!({ "ids": three, "response": approve, "note": "dentro do teto" });
    let (status, results) = send(&world, &bia, Method::POST, "/api/inbox/batch", Some(batch)).await;
    assert_eq!(status, StatusCode::OK, "{results}");
    assert!(
        results.as_array().is_some_and(|all| all.len() == 3 && all.iter().all(|one| one["ok"] == true)),
        "{results}"
    );
    let left = inbox_until(&world, &caio, 1, "approve").await;
    assert_eq!(left[0]["id"], id(&items[3]).as_str());
    let handed = inbox_until(&world, &root, 3, "handoff").await;
    assert_eq!(handed[0]["assignedTo"], json!({ "kind": "admins", "name": "" }));
    assert!(inbox(&world, &bia).await.iter().all(|entry| entry["item"]["step"] != "handoff"));
    let item = world.handle.item(&domain::workflow::ItemId::from(items[0].as_str())).expect("read").expect("item");
    let answered = item.inputs.iter().find(|input| input.step == "approve").expect("answered");
    assert_eq!(answered.payload["note"], "dentro do teto");
}

#[tokio::test(flavor = "multi_thread")]
async fn without_an_assignee_the_wait_stays_with_the_owner_and_a_release_frees_the_claim() {
    server::force_auth_mode(server::AuthMode::Shared);
    let world = World::new().await;
    let (ana, bia) = (signed(&world, "ana"), signed(&world, "bia"));
    give(&world, "bia", brand::access::ADMIN_ROLE);
    world.store.set_owner(store::ownership::CONNECTION, "c1", "ana").expect("owner");
    let made = json!({ "text": "Corrigir o login", "issue": null, "model": "auto", "attachments": [] });
    let (status, item) = send(&world, &ana, Method::POST, "/api/items", Some(made)).await;
    assert_eq!(status, StatusCode::OK, "{item}");
    let seen = inbox_until(&world, &ana, 1, "approve").await;
    assert!(seen[0]["assignedTo"].is_null(), "{seen:?}");
    assert!(inbox(&world, &bia).await.is_empty(), "an admin does not see an unassigned wait of someone else");
    let id = seen[0]["id"].clone();
    let (status, _) = send(&world, &ana, Method::POST, "/api/inbox/claim", Some(json!({ "id": id }))).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(inbox(&world, &ana).await[0]["claim"]["mine"], true);
    let (status, _) = send(&world, &bia, Method::POST, "/api/inbox/claim", Some(json!({ "id": id }))).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = send(&world, &ana, Method::POST, "/api/inbox/release", Some(json!({ "id": id }))).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert!(inbox(&world, &ana).await[0]["claim"].is_null());
}
