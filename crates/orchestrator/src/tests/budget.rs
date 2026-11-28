use domain::quota::Scope;
use serde_json::json;

use super::*;

fn spending(ceiling: &str) -> String {
    format!(
        "import {{ workflow }} from \"sdk\";\nexport default workflow({{ name: \"spending\", async run(ctx) {{ return (await ctx.agent(\"worker\", \"faça\", {{ budgetUsd: {ceiling} }})).text; }} }});"
    )
}

#[tokio::test(flavor = "multi_thread")]
async fn a_run_that_spends_past_its_ceiling_stops_the_item_and_says_which_ceiling() {
    let world = World::new();
    world.workflow("spending", &spending("0.3"));
    let kernel = world.start().await;
    let id = kernel.create_item("spending", json!({})).expect("item");
    let item = until(&kernel, &id, |item| item.impediment.is_some()).await;
    assert!(!item.is_ended());
    let reason = item.impediment.and_then(|found| found.reason).unwrap_or_default();
    assert_eq!(reason, "cost ceiling reached: run spent $0.40 of $0.30");
    let reached = crate::ceiling::budget_reached(&reason).expect("read back");
    assert_eq!(reached.scope, Scope::Run);
    let asked = world.engine.state.requests.lock().expect("requests");
    let budgets = &asked.last().expect("asked").policy.budgets;
    assert_eq!(budgets.iter().map(|b| (b.scope, b.limit_usd)).collect::<Vec<_>>(), [(Scope::Run, 0.3)]);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_run_within_its_ceiling_finishes_and_its_spend_is_kept_for_its_agent() {
    let world = World::new();
    world.workflow("spending", &spending("1"));
    let kernel = world.start().await;
    let id = kernel.create_item("spending", json!({})).expect("item");
    let item = ended(&kernel, &id).await;
    assert!(item.impediment.is_none());
    let kept = world.store().spent_since(0, Scope::Agent, "worker").expect("spend");
    assert_eq!(kept.known_micros, 400_000);
}

#[tokio::test(flavor = "multi_thread")]
async fn without_a_ceiling_a_run_is_given_no_budget_and_nothing_changes() {
    let world = World::new();
    world.workflow("plain", "import { workflow } from \"sdk\";\nexport default workflow({ name: \"plain\", async run(ctx) { return (await ctx.agent(\"worker\", \"faça\")).text; } });");
    let kernel = world.start().await;
    let id = kernel.create_item("plain", json!({})).expect("item");
    let item = ended(&kernel, &id).await;
    assert!(item.impediment.is_none());
    let asked = world.engine.state.requests.lock().expect("requests");
    assert!(asked.iter().all(|request| request.policy.budgets.is_empty()));
}
