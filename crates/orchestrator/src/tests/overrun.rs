use domain::quota::Scope;
use serde_json::json;

use super::*;

const REACHED: &str = "cost ceiling reached: run spent $0.40 of $0.30";

fn overrunning(on_budget: &str, extra: &str) -> String {
    format!(
        "import {{ workflow }} from \"sdk\";\nexport default workflow({{ name: \"overrunning\", async run(ctx) {{ return (await ctx.agent(\"worker\", \"faça\", {{ budgetUsd: 0.3, onBudget: {on_budget}{extra} }})).text; }} }});"
    )
}

async fn started(world: &World, on_budget: &str, extra: &str, input: Value) -> (Handle, ItemId) {
    world.workflow("overrunning", &overrunning(on_budget, extra));
    let kernel = world.start().await;
    let id = kernel.create_item("overrunning", input).expect("item");
    (kernel, id)
}

fn impeded_by(item: &WorkItem) -> String {
    item.impediment.as_ref().and_then(|found| found.reason.clone()).unwrap_or_default()
}

fn overrun_of(item: &WorkItem) -> Value {
    item.effects
        .values()
        .map(|entry| entry.progress["overrun"].clone())
        .find(|found| !found.is_null())
        .unwrap_or_default()
}

#[tokio::test(flavor = "multi_thread")]
async fn a_step_that_declares_stop_is_impeded_with_the_reason_as_without_a_declaration() {
    let world = World::new();
    let (kernel, id) = started(&world, "\"stop\"", "", json!({})).await;
    let item = until(&kernel, &id, |item| item.impediment.is_some()).await;
    assert!(!item.is_ended());
    assert_eq!(impeded_by(&item), REACHED);
    assert_eq!(overrun_of(&item), Value::Null, "stopping leaves the log as it always was");
    assert!(kernel.asks().is_empty());
    assert_eq!(world.engine.runs(), 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn asking_puts_the_overrun_on_the_item_and_the_run_goes_on_once_a_person_allows_it() {
    let world = World::new();
    let (kernel, id) = started(&world, "\"ask\"", "", json!({})).await;
    let asked = first_ask(&kernel).await;
    assert_eq!(asked.item, id);
    let AskKind::Permission { tool, subject, rememberable, .. } = &asked.ask.kind else {
        panic!("the overrun asks a permission: {asked:?}");
    };
    assert_eq!((tool.as_str(), subject.as_str(), *rememberable), ("budget", REACHED, false));
    assert_eq!(world.engine.runs(), 1, "nothing runs past the ceiling before the answer");
    kernel.answer_ask(&asked.ask.id, &Decision::Allow).expect("answer");
    let item = ended(&kernel, &id).await;
    assert!(item.impediment.is_none());
    assert!(output(&item).as_str().is_some_and(|text| text.starts_with("feito")), "{:?}", output(&item));
    let requests = world.engine.requests();
    let resumed = requests.last().expect("resumed");
    assert_eq!(resumed.prompt.text, crate::agent::RESUME);
    assert_eq!(resumed.session.resume, Some(SessionId("s1".into())));
    let budgets: Vec<(Scope, f64)> = resumed.policy.budgets.iter().map(|b| (b.scope, b.limit_usd)).collect();
    assert_eq!(budgets, [(Scope::Run, 0.6)], "the ceiling grows by itself once per approval");
    assert!(kernel.asks().is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_person_who_refuses_leaves_the_run_impeded_with_the_reason() {
    let world = World::new();
    let (kernel, id) = started(&world, "\"ask\"", "", json!({})).await;
    let asked = first_ask(&kernel).await;
    kernel.answer_ask(&asked.ask.id, &Decision::Deny).expect("answer");
    let item = until(&kernel, &id, |item| item.impediment.is_some()).await;
    assert_eq!(impeded_by(&item), REACHED);
    assert_eq!(overrun_of(&item)["went"], json!(false));
    assert_eq!(world.engine.runs(), 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_routine_has_no_one_to_ask_so_the_overrun_is_refused() {
    let world = World::new();
    let input = json!({ "schedule": { "subject": "schedule:overrunning:-:x" } });
    let (kernel, id) = started(&world, "\"ask\"", "", input).await;
    let item = until(&kernel, &id, |item| item.impediment.is_some()).await;
    assert_eq!(impeded_by(&item), REACHED);
    assert_eq!(overrun_of(&item), json!({ "action": "ask", "scope": "run", "went": false }));
    assert!(kernel.asks().is_empty());
    assert_eq!(world.engine.runs(), 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn falling_to_a_smaller_model_resumes_the_run_on_it_once() {
    agent_engine::providers::install_testing();
    let world = World::new();
    let requires = ", requires: { regions: [\"eu\"] }";
    let (kernel, id) = started(&world, "{ model: \"keyed-too:small\" }", requires, json!({})).await;
    let item = ended(&kernel, &id).await;
    assert!(item.impediment.is_none());
    let requests = world.engine.requests();
    assert_eq!(requests.len(), 2);
    let resumed = &requests[1];
    assert_eq!(resumed.session.model.id, "keyed-too:small");
    assert_eq!(resumed.prompt.text, crate::agent::RESUME);
    assert_eq!(resumed.policy.budgets.first().map(|b| b.limit_usd), Some(0.6));
    assert_eq!(overrun_of(&item)["model"], json!("keyed-too:small"));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_smaller_model_outside_the_data_policy_is_never_asked_and_the_run_stays_impeded() {
    agent_engine::providers::install_testing();
    let world = World::new();
    let requires = ", requires: { regions: [\"eu\"] }";
    let (kernel, id) = started(&world, "{ model: \"keyed:small\" }", requires, json!({})).await;
    let item = until(&kernel, &id, |item| item.impediment.is_some()).await;
    assert_eq!(impeded_by(&item), REACHED);
    let told = overrun_of(&item);
    assert_eq!(told["went"], json!(false));
    assert_eq!(told["skipped"], json!("keyed:small: region us-east-1 is not eu"));
    assert_eq!(world.engine.runs(), 1);
}
