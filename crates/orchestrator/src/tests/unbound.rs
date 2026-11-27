use domain::workflow::StepState;
use serde_json::json;

use super::*;

const UNBOUND: &str = r#"
import { workflow, step } from "sdk";
import { ticket, cases } from "sdk/steps";
const planned = step({ name: "planned", async run(ctx) { return ctx.pass({ title: "Tela de pedidos" }); } });
export default workflow({ name: "unbound", steps: [planned({ status: { category: "todo" } }), ticket(), cases({ agent: "tester" })] });
"#;

fn tracker_calls(world: &World) -> Vec<String> {
    let calls = world.provider.calls.lock().expect("lock");
    calls.iter().filter(|(name, _, _)| name.starts_with("tracker.")).map(|(name, _, _)| name.clone()).collect()
}

#[tokio::test(flavor = "multi_thread")]
async fn an_application_without_a_tracker_or_test_project_skips_those_steps_and_never_touches_the_tools() {
    let world = World::new();
    world.workflow("unbound", UNBOUND);
    world.provider.then("tracker.schema", Reply::Fail(ActionError::blocked("Choose the project.")));
    let kernel = world.start().await;
    let input = json!({ "title": "Tela de pedidos", "application": { "id": "app", "name": "XPTO", "bindings": {}, "repositories": [] } });
    let id = kernel.create_item("unbound", input).expect("item");
    let item = until(&kernel, &id, |item| item.is_ended() || item.impediment.is_some() || item.waiting_human()).await;
    assert!(item.impediment.is_none(), "{:?}", item.impediment);
    assert!(item.is_ended(), "{:?}", item.steps);
    assert_eq!(item.step("planned", Some("main")).map(|run| run.state), Some(StepState::Passed));
    assert_eq!(item.step("ticket", Some("main")).map(|run| run.state), Some(StepState::Skipped));
    assert_eq!(item.step("cases", Some("main")).map(|run| run.state), Some(StepState::Skipped));
    assert_eq!(tracker_calls(&world), Vec::<String>::new());
}

#[tokio::test(flavor = "multi_thread")]
async fn an_application_bound_to_a_tracker_project_still_creates_the_issue_there() {
    let world = World::new();
    world.workflow("unbound", UNBOUND);
    world.provider.always("tracker.create_issue", json!({ "ref": "XP-1" }));
    let kernel = world.start().await;
    let bindings = json!({ "tracker": { "connection": "c1", "value": "XP" } });
    let input = json!({ "title": "Tela de pedidos", "project": "XP", "connection": "c1",
        "application": { "id": "app", "name": "XPTO", "bindings": bindings, "repositories": [] } });
    let id = kernel.create_item("unbound", input).expect("item");
    until(&kernel, &id, |item| item.step("ticket", Some("main")).is_some_and(|run| run.state == StepState::Passed))
        .await;
    let created =
        world.provider.calls.lock().expect("lock").iter().find(|(name, _, _)| name == "tracker.create_issue").cloned();
    assert_eq!(created.map(|(_, args, _)| args["project"].clone()), Some(json!("XP")));
}

const LATER: &str = r#"
import { workflow, step } from "sdk";
import { ticket } from "sdk/steps";
const waiting = step({ name: "waiting", async run(ctx) { await ctx.waitHuman([]); return ctx.pass({ seen: Object.keys(ctx.item.application.bindings || {}) }); } });
export default workflow({ name: "later", steps: [waiting(), ticket()] });
"#;

#[tokio::test(flavor = "multi_thread")]
async fn a_tracker_project_bound_while_the_item_runs_is_used_by_the_steps_that_start_after_it() {
    let world = World::new();
    world.workflow("later", LATER);
    world.provider.always("tracker.create_issue", json!({ "ref": "XP-7" }));
    let mut application = store::Application { id: "app".into(), name: "XPTO".into(), ..store::Application::default() };
    world.store().save_application(&application).expect("app");
    let kernel = world.start().await;
    let input = json!({ "title": "Tela de pedidos", "application": { "id": "app", "name": "XPTO", "bindings": {}, "repositories": [] } });
    let id = kernel.create_item("later", input).expect("item");
    until(&kernel, &id, WorkItem::waiting_human).await;
    application.bindings = store::Bindings::from([("tracker".to_owned(), store::Binding::new("c1", "XP"))]);
    world.store().save_application(&application).expect("bound");
    kernel.human_input(&id, "waiting", "ok", json!({})).expect("answer");
    let item = until(&kernel, &id, |item| item.is_ended() || item.impediment.is_some()).await;
    assert_eq!(item.step("waiting", Some("main")).map(|run| run.output["seen"].clone()), Some(json!([])));
    assert_eq!(item.step("ticket", Some("main")).map(|run| run.state), Some(StepState::Passed), "{:?}", item.steps);
    let created =
        world.provider.calls.lock().expect("lock").iter().find(|(name, _, _)| name == "tracker.create_issue").cloned();
    assert_eq!(created.map(|(_, args, _)| args["project"].clone()), Some(json!("XP")));
}
