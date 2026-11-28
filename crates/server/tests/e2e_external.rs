#![allow(clippy::expect_used, clippy::panic, reason = "a failed step stops the test")]

mod e2e;

use domain::workflow::{ItemId, WorkflowEvent};
use e2e::{Hub, Server, step, waiting_on};
use serde_json::{Value, json};

fn new_item(text: &str) -> Value {
    json!({ "text": text, "issue": null, "model": "auto", "attachments": [] })
}

fn reconciled(server: &Server, id: &str) -> Vec<(String, String)> {
    let events = server.store.timed_events(&ItemId::from(id), 0).expect("events");
    events
        .into_iter()
        .filter_map(|(_, _, event)| match event {
            WorkflowEvent::Reconciled { field, action, .. } => Some((field, action)),
            _ => None,
        })
        .collect()
}

fn observed(server: &Server, id: &str) -> Vec<String> {
    let events = server.store.timed_events(&ItemId::from(id), 0).expect("events");
    events
        .into_iter()
        .filter_map(|(_, _, event)| match event {
            WorkflowEvent::ExternalObserved(seen) => Some(seen.kind),
            _ => None,
        })
        .collect()
}

async fn eventually(what: &str, check: impl Fn() -> bool) {
    assert!(e2e::eventually(|| check().then_some(())).await.is_some(), "{what} never happened");
}

async fn to_review(server: &Server, text: &str) -> String {
    let id = server.create(new_item(text)).await;
    server.approve_plan(&id).await;
    server.until(&id, "the review waits", |item| waiting_on(item, "review")).await;
    id
}

#[tokio::test(flavor = "multi_thread")]
async fn an_issue_moved_outside_is_put_back_and_a_deploy_approved_outside_lets_the_item_finish() {
    let hub = Hub::new();
    let server = hub.start().await;
    let id = to_review(&server, "Mudanças por fora").await;
    assert_eq!(hub.tracker.statuses_of("K-101"), ["Fazendo"]);
    hub.tracker.move_outside("K-101", "A fazer");
    eventually("the status is put back", || hub.tracker.statuses_of("K-101").len() == 2).await;
    assert_eq!(hub.tracker.statuses_of("K-101"), ["Fazendo", "Fazendo"]);
    assert_eq!(hub.tracker.state().issues["K-101"].status, "Fazendo");
    assert!(
        reconciled(&server, &id).contains(&("status".into(), "reapplied".into())),
        "{:?}",
        reconciled(&server, &id)
    );

    server.input(&id, "review", "review", json!({ "verdict": "approve", "body": "", "notes": [] })).await;
    server.until(&id, "the merge waits", |item| waiting_on(item, "merge")).await;
    server.input(&id, "merge", "merge", json!({ "method": "squash" })).await;
    server.until(&id, "the deploy waits", |item| waiting_on(item, "release")).await;
    hub.github.approve_deploy();
    let done = server.until(&id, "the delivery", |item| item["group"] == "delivered").await;
    assert_eq!(done["status"], "Feito");
    assert!(hub.github.state().deploy_reviews.is_empty(), "nobody approved through the API");
    assert_eq!(hub.tracker.state().created, 1);
    assert_eq!(hub.tracker.state().comments_posted, 1, "only the summary of the E2E tests");
    let comments = hub.tracker.state().issues["K-101"].comments.clone();
    assert!(comments[0].contains("K-R1"), "{comments:?}");
    assert_eq!(hub.github.pulls_created(), 1);
    assert_eq!(hub.github.state().merges, 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_pull_request_closed_outside_reaches_the_item_by_event_and_the_merge_stops_with_you() {
    let hub = Hub::new();
    let server = hub.start().await;
    let id = to_review(&server, "PR fechado").await;
    hub.github.close_pull(1);
    e2e::eventually(|| observed(&server, &id).iter().any(|kind| kind == "code.pr.closed").then_some(())).await;
    assert!(observed(&server, &id).iter().any(|kind| kind == "code.pr.closed"), "{:?}", observed(&server, &id));
    server.input(&id, "review", "review", json!({ "verdict": "approve", "body": "", "notes": [] })).await;
    server.until(&id, "the merge waits", |item| waiting_on(item, "merge")).await;
    server.input(&id, "merge", "merge", json!({ "method": "squash" })).await;
    let stuck = server.until(&id, "the merge stops", |item| item["impediment"].is_object()).await;
    assert_eq!(stuck["group"], "you");
    assert!(stuck["impediment"]["reason"].as_str().is_some_and(|reason| reason.starts_with("Merge failed")), "{stuck}");
    assert!(
        ["failed", "waiting_human"].contains(&step(&stuck, "merge")["state"].as_str().unwrap_or_default()),
        "{stuck}"
    );
    assert_eq!(hub.github.state().merges, 0);
    assert_eq!(hub.github.pulls_created(), 1);
    assert_eq!(hub.tracker.state().created, 1);
}

const LEAN: &str = r#"
import { workflow } from "sdk";
import { plan, ticket, develop, pr, merge } from "sdk/steps";

export default workflow({
  name: "enxuto",
  statuses: ["Etapa 1", "Etapa 2", "Etapa 3"],
  steps: [
    plan({ agent: "planner", status: "Etapa 1" }),
    ticket(),
    develop({ agent: "developer", status: "Etapa 2" }),
    pr(),
    merge({ method: "squash", doneStatus: "Etapa 3" }),
  ],
});
"#;

#[tokio::test(flavor = "multi_thread")]
async fn a_new_workflow_in_the_repository_changes_the_steps_and_the_statuses_sent_to_the_tracker() {
    let hub = Hub::new();
    let server = hub.start().await;
    let old = to_review(&server, "Antes da troca").await;
    server.stop().await;

    hub.write("workflows/default.ts", LEAN);
    let server = hub.start().await;
    let kept = server.until(&old, "the old item keeps its own workflow", |item| waiting_on(item, "review")).await;
    assert!(kept["impediment"].is_null(), "{kept}");
    assert!(kept["steps"].as_array().is_some_and(|steps| steps.iter().any(|step| step["id"] == "ci")), "{kept}");
    let id = server.create(new_item("Depois da troca")).await;
    let created = server.item(&id).await;
    let ids: Vec<&str> =
        created["steps"].as_array().expect("steps").iter().filter_map(|step| step["id"].as_str()).collect();
    assert_eq!(ids, ["plan", "ticket", "develop", "pr", "merge"]);
    server.approve_plan(&id).await;
    let merging = server.until(&id, "the merge waits", |item| waiting_on(item, "merge")).await;
    assert_eq!(merging["status"], "Etapa 2");
    assert_eq!(merging["change"]["checks"], Value::Null, "this workflow has no pipeline step");
    let view = server.get(&format!("/api/items/{id}/steps/pr")).await;
    assert_eq!(view["blocks"][0]["type"], "change", "an opened pull request is shown live");
    assert_eq!(view["blocks"][0]["change"], merging["change"]["key"]);
    server.input(&id, "merge", "merge", json!({ "method": "squash" })).await;
    let done = server.until(&id, "the delivery", |item| item["group"] == "delivered").await;
    assert_eq!(done["status"], "Etapa 3");
    assert_eq!(done["change"]["key"], merging["change"]["key"], "a workflow without output inherits the pull request of its pr step");
    assert_eq!(done["change"]["merged"], true);
    assert_eq!((done["delivered"]["kind"].clone(), done["delivered"]["done"].clone()), (json!("pr"), json!(true)));
    let sent = hub.tracker.statuses_of("K-102");
    assert!(sent.ends_with(&["Etapa 2".to_owned(), "Etapa 3".to_owned()]), "{sent:?}");
    assert!(sent.iter().all(|status| status.starts_with("Etapa")), "{sent:?}");
    assert_eq!(hub.github.state().merges, 1);
}
