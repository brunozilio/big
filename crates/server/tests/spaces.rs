#![allow(clippy::expect_used, clippy::panic, reason = "a failed step stops the test")]

mod support;

use std::sync::Arc;

use axum::http::{Method, StatusCode};
use domain::workflow::{ItemId, StepState};
use serde_json::{Value, json};
use store::ConnectionRecord;
use support::{Extra, World};

const ERP: &str = "connectors/mock_purchasing/index.ts";

async fn with_an_erp() -> World {
    connectors::memory::install(Arc::new(connectors::memory::MemoryValues::default()));
    let erp = connectors::scripted::ScriptedProvider::load_in(None, ERP).expect("the simulated purchasing system");
    let record = ConnectionRecord {
        id: "c-erp".into(),
        provider: "mock-purchasing".into(),
        label: "Compras".into(),
        account: Some("Financeiro".into()),
        capabilities: vec!["purchasing".into()],
        settings: json!({}),
    };
    World::equipped(Extra { providers: vec![Arc::new(erp)], connections: vec![record] }).await
}

fn finance() -> Value {
    json!({
        "name": "Financeiro",
        "bindings": { "purchasing": { "connection": "c-erp", "value": "",
            "fields": { "company": { "value": "acme", "label": "ACME Indústria" } } } },
        "repositories": [],
        "workflow": " invoice-match ",
        "skills": ["review", " ", "review"],
        "instructions": " Só libere nota com pedido aprovado. ",
    })
}

#[tokio::test(flavor = "multi_thread")]
async fn a_finance_space_with_an_erp_and_no_repository_runs_an_invoice_from_start_to_end() {
    let world = with_an_erp().await;
    let (status, space) = world.send(Method::POST, "/api/applications", Some(finance())).await;
    assert_eq!(status, StatusCode::OK, "{space}");
    assert_eq!(space["repositories"], json!([]));
    assert_eq!((&space["workflow"], &space["skills"]), (&json!("invoice-match"), &json!(["review"])));
    assert_eq!(space["instructions"], "Só libere nota com pedido aprovado.");
    let item = json!({ "text": "Conferir a NF 4521", "issue": null, "model": "auto", "attachments": [], "application": space["id"] });
    let (status, created) = world.send(Method::POST, "/api/items", Some(item)).await;
    assert_eq!(status, StatusCode::OK, "{created}");
    assert_eq!(created["workflow"], "invoice-match", "the space's default workflow");
    let id = created["id"].as_str().expect("id").to_owned();
    world.until("the approval waits", |found| found["id"] == id.as_str() && found["current"] == "approval").await;
    let approve = json!({ "action": "approve", "payload": {} });
    let (status, said) =
        world.send(Method::POST, &format!("/api/items/{id}/steps/approval/input"), Some(approve)).await;
    assert_eq!(status, StatusCode::NO_CONTENT, "{said}");
    let mut done = None;
    for _ in 0..400 {
        done = world.handle.item(&ItemId::from(id.as_str())).expect("store").filter(|item| item.is_ended());
        if done.is_some() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }
    let done = done.expect("the item ends");
    for step in ["documents", "match", "approval", "record", "deliver"] {
        assert_eq!(
            done.step(step, Some("main")).map(|run| run.state),
            Some(StepState::Passed),
            "{step}: {:?}",
            done.steps
        );
    }
    let released = &done.step("deliver", Some("main")).expect("deliver").output;
    assert!(released.to_string().contains("PAG-4521"), "{released}");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_space_refuses_a_default_workflow_or_a_skill_that_does_not_exist_and_lists_its_own_items() {
    let world = with_an_erp().await;
    let mut wrong = finance();
    wrong["workflow"] = json!("nao-existe");
    assert_eq!(world.send(Method::POST, "/api/applications", Some(wrong)).await.0, StatusCode::BAD_REQUEST);
    let mut unknown = finance();
    unknown["skills"] = json!(["review", "conciliacao"]);
    let (status, refused) = world.send(Method::POST, "/api/applications", Some(unknown)).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(refused["error"]["message"].as_str().is_some_and(|text| text.contains("conciliacao")), "{refused}");
    let space = world.send(Method::POST, "/api/applications", Some(finance())).await.1;
    let mine = json!({ "text": "Conferir a NF 4533", "issue": null, "model": "auto", "attachments": [], "application": space["id"] });
    let loose = json!({ "text": "Revisar o contrato", "issue": null, "model": "auto", "attachments": [], "workflow": "invoice-match" });
    for body in [mine, loose] {
        assert_eq!(world.send(Method::POST, "/api/items", Some(body)).await.0, StatusCode::OK);
    }
    let id = space["id"].as_str().expect("id");
    let listed = world.get(&format!("/api/items?application={id}")).await;
    let titles: Vec<&str> = listed.as_array().into_iter().flatten().filter_map(|item| item["title"].as_str()).collect();
    assert_eq!(titles, ["Conferir a NF 4533"]);
    assert_eq!(world.get("/api/items").await.as_array().map(Vec::len), Some(2));
}
