#![allow(clippy::expect_used, clippy::panic, reason = "a failed step stops the test")]

mod e2e;

use std::sync::atomic::Ordering;

use e2e::{Hub, Server, step, waiting_on};
use serde_json::json;

async fn approve_plan(server: &Server, id: &str) {
    let ask = server.ask(id, "the plan").await;
    assert!(matches!(ask.ask.kind, domain::AskKind::Plan { .. }), "{ask:?}");
    let item = server.until(id, "the plan asks", |item| item["group"] == "you").await;
    assert_eq!(item["attention"]["badge"]["text"], "Plano pronto", "a plan to approve is not a question");
    server.respond(&ask, json!({ "kind": "plan", "decision": "approve" })).await;
    server.approve_cases(id).await;
}

async fn approve_review_merge_and_deploy(server: &Server, id: &str) {
    server.until(id, "the review waits", |item| waiting_on(item, "review")).await;
    server.input(id, "review", "review", json!({ "verdict": "approve", "body": "ok", "notes": [] })).await;
    server.until(id, "the merge waits", |item| waiting_on(item, "merge")).await;
    server.input(id, "merge", "merge", json!({ "method": "squash" })).await;
    server.until(id, "the deploy waits", |item| waiting_on(item, "release")).await;
    server.input(id, "release", "approve_deploy", json!({})).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn an_item_without_an_issue_goes_from_the_plan_to_production_through_the_api() {
    let hub = Hub::new();
    hub.github.fail_next_ci();
    hub.agents.script.ask_develop.store(true, Ordering::SeqCst);
    let server = hub.start().await;
    let id = server.create(json!({ "text": "Adicionar o arquivo de trabalho\nDetalhes", "issue": null, "model": "auto", "attachments": [] })).await;
    approve_plan(&server, &id).await;

    let asked = server.ask(&id, "a permission while developing").await;
    assert!(matches!(asked.ask.kind, domain::AskKind::Permission { .. }), "{asked:?}");
    let item = server.until(&id, "the develop ask", |item| item["group"] == "you").await;
    assert_eq!(item["issue"]["key"], "K-101");
    assert_eq!(item["current"], "develop");
    server.respond(&asked, json!({ "kind": "permission", "decision": "allowed" })).await;
    let session = server.get(&format!("/api/sessions/{}", asked.session.clone().expect("session"))).await;
    assert_eq!(session["agents"][0]["name"], "Developer", "{session}");
    assert_eq!(session["agents"][0]["avatar"]["kind"], "agent");
    assert!(
        session["events"].as_array().is_some_and(|events| events
            .iter()
            .any(|event| event["type"] == "permission" && event["resolution"] == "allowed")),
        "{session}"
    );

    let review = server.until(&id, "the review waits", |item| waiting_on(item, "review")).await;
    assert_eq!(step(&review, "ci")["attempts"], 2, "the failed CI went back to develop: {review}");
    assert_eq!(step(&review, "develop")["attempts"], 2);
    assert_eq!(review["change"]["key"], "acme/app#1");
    assert_eq!(review["change"]["checks"], "passed");
    assert_eq!(review["status"], "Fazendo");
    let notes = json!([{ "file": "work-2.txt", "line": 1, "body": "use um nome melhor" }]);
    server.input(&id, "review", "review", json!({ "verdict": "changes", "body": "ajuste", "notes": notes })).await;
    server.until(&id, "the review comes back", |item| step(item, "develop")["attempts"] == 3).await;
    let fixes = e2e::eventually(|| Some(hub.agents.runs_of("developer")).filter(|runs| runs.len() >= 3))
        .await
        .unwrap_or_else(|| hub.agents.runs_of("developer"));
    assert!(fixes.last().is_some_and(|(_, prompt)| prompt.contains("use um nome melhor")), "{fixes:?}");

    approve_review_merge_and_deploy(&server, &id).await;
    let done = server.until(&id, "the delivery", |item| item["group"] == "delivered").await;
    assert_eq!(done["status"], "Feito");
    assert_eq!(done["change"]["merged"], true);

    assert_eq!(hub.tracker.state().created, 1);
    assert_eq!(hub.tracker.statuses_of("K-101"), ["Fazendo", "Feito"]);
    assert_eq!(hub.github.pulls_created(), 1);
    let reviews = hub.github.reviews();
    let asked_changes = reviews.iter().find(|review| review.state == "CHANGES_REQUESTED").expect("changes");
    assert_eq!(asked_changes.lines[0]["path"], "work-2.txt");
    assert_eq!(asked_changes.lines[0]["line"], 1);
    assert!(reviews.iter().any(|review| review.state == "APPROVED"));
    let resolved = hub.github.state().resolved.clone();
    assert!(!resolved.is_empty(), "the approval resolves the threads the developer was handed");
    let open = hub.github.open_threads();
    assert!(open.is_empty(), "every thread handed to the developer is resolved: {open:?}");
    assert_eq!(hub.github.state().merges, 1);
    assert_eq!(hub.github.state().deploy_reviews, ["approved"]);
    let steps = server.get(&format!("/api/items/{id}/steps/ci")).await;
    assert_eq!(steps["attempts"].as_array().map(Vec::len), Some(3));
    assert_eq!(steps["attempts"][0]["state"], "failed");
}

#[tokio::test(flavor = "multi_thread")]
async fn an_item_from_an_existing_issue_moves_it_through_every_status_without_creating_another() {
    let hub = Hub::new();
    hub.tracker.add_issue("K-7", "Corrigir o login", "Backlog");
    let server = hub.start().await;
    let body =
        json!({ "text": "", "issue": { "connection": "trk", "key": "K-7" }, "model": "auto", "attachments": [] });
    let id = server.create(body).await;
    let created = server.item(&id).await;
    assert_eq!((created["title"].as_str(), created["issue"]["key"].as_str()), (Some("K-7"), Some("K-7")));
    approve_plan(&server, &id).await;
    approve_review_merge_and_deploy(&server, &id).await;
    server.until(&id, "the delivery", |item| item["group"] == "delivered").await;
    assert_eq!(hub.tracker.state().created, 0);
    assert_eq!(hub.tracker.statuses_of("K-7"), ["A fazer", "Fazendo", "Feito"]);
}

async fn release_view_with(server: &Server, id: &str, kind: &str) -> serde_json::Value {
    let deadline = tokio::time::Instant::now() + e2e::PATIENCE;
    while tokio::time::Instant::now() < deadline {
        let view = server.get(&format!("/api/items/{id}/steps/release")).await;
        if view["blocks"].as_array().is_some_and(|blocks| blocks.iter().any(|block| block["type"] == kind)) {
            return view;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    panic!("the release never showed a {kind} block")
}

#[tokio::test(flavor = "multi_thread")]
async fn a_rejected_deploy_asks_why_with_a_form_and_the_reason_reaches_the_pipeline_and_the_item() {
    let hub = Hub::new();
    hub.tracker.add_issue("K-7", "Corrigir o login", "Backlog");
    let server = hub.start().await;
    let body =
        json!({ "text": "", "issue": { "connection": "trk", "key": "K-7" }, "model": "auto", "attachments": [] });
    let id = server.create(body).await;
    approve_plan(&server, &id).await;
    server.until(&id, "the review waits", |item| waiting_on(item, "review")).await;
    server.input(&id, "review", "review", json!({ "verdict": "approve", "body": "ok", "notes": [] })).await;
    server.until(&id, "the merge waits", |item| waiting_on(item, "merge")).await;
    server.input(&id, "merge", "merge", json!({ "method": "squash" })).await;
    server.until(&id, "the deploy waits", |item| waiting_on(item, "release")).await;
    release_view_with(&server, &id, "actions").await;
    server.input(&id, "release", "reject_deploy", json!({})).await;

    let view = release_view_with(&server, &id, "form").await;
    let form =
        view["blocks"].as_array().and_then(|blocks| blocks.iter().find(|block| block["type"] == "form")).expect("form");
    assert_eq!(form["title"], "Rejection reason");
    assert_eq!(form["submit"], json!({ "action": "submit", "label": "Reject deploy" }));
    assert_eq!(form["fields"][0]["name"], "reason");
    assert_eq!(form["fields"][0]["kind"], "textarea");
    server.input(&id, "release", "submit", json!({ "reason": "janela de congelamento" })).await;

    let stuck = server
        .until(&id, "the release stops with the reason", |item| {
            item["impediment"]["detail"].as_str().is_some_and(|detail| detail.contains("janela de congelamento"))
        })
        .await;
    assert_eq!(stuck["group"], "you");
    assert_eq!(hub.github.state().deploy_reviews, ["rejected"]);
    assert_eq!(hub.github.state().deploy_comments, ["janela de congelamento"]);
}
