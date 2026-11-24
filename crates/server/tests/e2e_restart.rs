#![allow(clippy::expect_used, clippy::panic, reason = "a failed step stops the test")]

mod e2e;

use std::sync::atomic::Ordering;
use std::time::Duration;

use axum::http::{Method, StatusCode};
use e2e::{Hub, step, waiting_on};
use serde_json::{Value, json};

fn new_item() -> Value {
    json!({ "text": "Reiniciar no meio", "issue": null, "model": "auto", "attachments": [] })
}

async fn until_true(what: &str, check: impl Fn() -> bool) {
    for _ in 0..1000 {
        if check() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("{what} never happened");
}

async fn until_true_async<F, Fut>(what: &str, check: F)
where
    F: Fn() -> Fut,
    Fut: std::future::Future<Output = bool>,
{
    for _ in 0..500 {
        if check().await {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("{what} never happened");
}

#[tokio::test(flavor = "multi_thread")]
async fn what_a_running_step_writes_reaches_its_session_before_the_run_ends() {
    let hub = Hub::new();
    hub.agents.script.hold_develop.store(true, Ordering::SeqCst);
    let server = hub.start().await;
    let id = server.create(new_item()).await;
    server.approve_plan(&id).await;
    let mut session = None;
    for _ in 0..500 {
        let view = server.get(&format!("/api/items/{id}/steps/develop")).await;
        session = view["blocks"][0]["session"].as_str().map(str::to_owned);
        if session.is_some() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let session = session.expect("the develop session");
    until_true_async("the text written so far is shown", || async {
        let shown = server.get(&format!("/api/sessions/{session}")).await;
        shown["status"] == "running" && shown["events"].to_string().contains("trabalhando")
    })
    .await;
    let message = json!({ "text": "inclua os testes", "model": null, "attachments": [] });
    let (status, _) = server.send(Method::POST, &format!("/api/sessions/{session}/messages"), Some(message)).await;
    assert_eq!(status, StatusCode::NO_CONTENT, "a message to a running step is taken, not refused");
    let steered = hub.agents.script.steered.lock().expect("steered").clone();
    assert_eq!(steered, ["inclua os testes"], "the message reaches the running agent");
    hub.agents.script.hold_develop.store(false, Ordering::SeqCst);
    server.until(&id, "develop goes on", |item| item["current"] != "develop").await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_restart_in_the_middle_of_develop_resumes_the_session_without_repeating_effects() {
    let hub = Hub::new();
    hub.agents.script.hold_develop.store(true, Ordering::SeqCst);
    let server = hub.start().await;
    let id = server.create(new_item()).await;
    server.approve_plan(&id).await;
    until_true("the developer starts", || hub.agents.runs_of("developer").len() == 1).await;
    let before = server.until(&id, "develop runs", |item| item["current"] == "develop").await;
    assert_eq!(before["live"], true);
    until_true_async("the develop session is recorded", || async {
        server.get(&format!("/api/items/{id}/steps/develop")).await.to_string().contains("developer-")
    })
    .await;
    server.stop().await;

    hub.agents.script.hold_develop.store(false, Ordering::SeqCst);
    let server = hub.start().await;
    let after = server.item(&id).await;
    assert_eq!((after["current"].as_str(), after["status"].as_str()), (Some("develop"), Some("Fazendo")));
    server.until(&id, "the review after the restart", |item| waiting_on(item, "review")).await;
    let runs = hub.agents.runs_of("developer");
    assert_eq!(runs.len(), 2, "{runs:?}");
    assert_eq!(runs[0].0, runs[1].0, "the same session was resumed");
    assert!(runs[1].1.contains("interrompida"), "{runs:?}");
    assert_eq!(hub.agents.runs_of("planner").len(), 1, "the plan is not redone");
    assert_eq!(hub.tracker.state().created, 1);
    assert_eq!(hub.github.pulls_created(), 1);
    assert_eq!(hub.tracker.statuses_of("K-101"), ["Fazendo"]);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_pending_ask_of_the_agent_comes_back_after_a_restart_and_is_answered_through_the_api() {
    let hub = Hub::new();
    hub.agents.script.ask_develop.store(true, Ordering::SeqCst);
    let server = hub.start().await;
    let id = server.create(new_item()).await;
    server.approve_plan(&id).await;
    let asked = server.ask(&id, "a permission").await;
    assert!(matches!(asked.ask.kind, domain::AskKind::Permission { .. }));
    server.stop().await;

    hub.agents.script.hold_develop.store(true, Ordering::SeqCst);
    let server = hub.start().await;
    let back = server.asks(&id).await;
    assert_eq!(back.len(), 1, "{back:?}");
    assert_eq!((back[0].ask.id.clone(), back[0].session.clone()), (asked.ask.id.clone(), asked.session.clone()));
    let item = server.until(&id, "the item is with you again", |item| item["group"] == "you").await;
    assert_eq!(item["attention"]["badge"]["text"], "Pergunta do agente");
    server.respond(&back[0], json!({ "kind": "permission", "decision": "allowed" })).await;
    assert!(server.asks(&id).await.is_empty());
    hub.agents.script.hold_develop.store(false, Ordering::SeqCst);
    server.until(&id, "the review", |item| waiting_on(item, "review")).await;
    let session = asked.session.clone().expect("session");
    server.stop().await;

    let server = hub.start().await;
    let shown = server.get(&format!("/api/sessions/{session}")).await;
    let answered = shown["events"]
        .as_array()
        .and_then(|events| events.iter().find(|event| event["id"] == asked.ask.id.0.as_str()))
        .cloned();
    assert_eq!(answered.map(|event| event["resolution"].clone()), Some(json!("allowed")), "{shown}");
    assert!(server.asks(&id).await.is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_restart_while_the_pipeline_runs_waits_for_the_same_run_and_goes_on() {
    let hub = Hub::new();
    hub.github.state().ci_running = true;
    let server = hub.start().await;
    let id = server.create(new_item()).await;
    server.approve_plan(&id).await;
    server.until(&id, "the pipeline runs", |item| step(item, "ci")["state"] == "running").await;
    until_true("the run exists", || hub.github.ci_runs() == 1).await;
    let checks = server.item(&id).await;
    assert_eq!(checks["change"]["checks"], "running");
    server.stop().await;

    hub.github.finish_ci("success");
    let server = hub.start().await;
    let item = server.until(&id, "the review after the pipeline", |item| waiting_on(item, "review")).await;
    assert_eq!(item["change"]["checks"], "passed");
    assert_eq!(step(&item, "ci")["attempts"], 1);
    assert_eq!(hub.github.ci_runs(), 1);
    assert_eq!(hub.github.pulls_created(), 1);
    assert_eq!(hub.tracker.state().created, 1);
    assert_eq!(hub.agents.runs_of("developer").len(), 1);
}

const FANOUT: &str = r#"
import { workflow } from "sdk";

export default workflow({
  name: "default",
  async run(ctx, item) {
    await ctx.status("Fazendo");
    const [child, moved] = await ctx.all([
      ctx.child("kid", { from: item.id }),
      ctx.event("tracker.status.changed", { subject: item.issue, to: "Pronto" }, { timeout: 120000 }),
    ]);
    const late = await ctx.event("tracker.status.changed", { subject: "K-404" }, { timeout: 300 }).then(() => "chegou", (error) => error.kind);
    await ctx.call("tracker", "comment", { issue: item.issue, body: `filho: ${child}` });
    return { child, to: moved.data.to, late };
  },
});
"#;

const KID: &str = r#"
import { workflow } from "sdk";

export default workflow({
  name: "kid",
  async run(ctx) {
    const answer = await ctx.waitHuman([]);
    return answer.action;
  },
});
"#;

#[tokio::test(flavor = "multi_thread")]
async fn a_custom_workflow_with_all_child_and_event_timeouts_resumes_after_a_restart() {
    let hub = Hub::new();
    hub.write("workflows/default.ts", FANOUT);
    hub.write("workflows/kid.ts", KID);
    hub.tracker.add_issue("K-9", "Paralelo", "A fazer");
    let server = hub.start().await;
    let body = json!({ "text": "Paralelo", "issue": { "connection": "trk", "key": "K-9" }, "model": "auto", "attachments": [] });
    let id = server.create(body).await;
    let parent = domain::workflow::ItemId::from(id.as_str());
    let mut child = None;
    for _ in 0..500 {
        child = server
            .handle
            .item(&parent)
            .expect("store")
            .and_then(|item| item.children.first().map(|link| link.id.clone()));
        if child.is_some() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let child = child.expect("the child started");
    until_true("the tracker is polled", || hub.tracker.state().polls >= 2).await;
    server.stop().await;

    hub.tracker.move_outside("K-9", "Pronto");
    let server = hub.start().await;
    for _ in 0..500 {
        if server.handle.item(&child).expect("store").is_some_and(|kid| kid.pending_effects().count() == 1) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    server.handle.human_input(&child, "", "go", json!({})).expect("the child answers");
    let done = server.until(&id, "the parent ends", |item| item["group"] == "delivered").await;
    assert_eq!(done["status"], "Fazendo");
    let ended = server.handle.item(&parent).expect("store").expect("item");
    let Some(domain::workflow::Ending::Finished { output }) = ended.ending else { panic!("{ended:?}") };
    assert_eq!(output, json!({ "child": "go", "to": "Pronto", "late": "timeout" }));
    assert_eq!(hub.tracker.state().comments_posted, 1);
    server.stop().await;
    let server = hub.start().await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(hub.tracker.state().comments_posted, 1, "a restart after the end repeats nothing");
    assert_eq!(server.item(&id).await["group"], "delivered");
}
