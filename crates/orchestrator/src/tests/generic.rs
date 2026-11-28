use domain::workflow::StepState;
use serde_json::json;

use super::*;

const ANSWER: &str = r#"
import { workflow } from "sdk";
import { draft, review, act, deliver } from "sdk/steps/general.js";

export default workflow({
  name: "resposta",
  steps: [
    draft({ agent: "worker" }),
    review({ onChanges: "draft" }),
    act({ capability: "changes", action: "get", args: { change: "GMUD-1" } }),
    deliver({ capability: "tracker", action: "comment", args: (item, ctx) => ({ issue: "SAC-12", body: ctx.output("draft").draft }) }),
  ],
});
"#;

fn ran(world: &World, prefix: &str) -> usize {
    world.provider.calls.lock().expect("lock").iter().filter(|(name, _, _)| name.starts_with(prefix)).count()
}

#[tokio::test(flavor = "multi_thread")]
async fn a_draft_is_reviewed_acted_on_and_delivered_with_no_code_connector() {
    let world = World::new();
    world.workflow("resposta", ANSWER);
    let kernel = world.start().await;
    let id = kernel.create_item("resposta", json!({ "title": "Pedido 881 chegou incompleto" })).expect("item");
    until(&kernel, &id, |item| item.waiting_human()).await;
    kernel.human_input(&id, "review", "submit", json!({ "body": "Cite o prazo" })).expect("changes");
    until(&kernel, &id, |item| item.waiting_human() && world.engine.runs() == 2).await;
    assert!(world.engine.requests()[1].prompt.text.contains("Cite o prazo"));
    kernel.human_input(&id, "review", "approve", json!({})).expect("approve");
    let item = ended(&kernel, &id).await;
    for step in ["draft", "review", "act", "deliver"] {
        assert_eq!(item.step(step, Some("main")).map(|run| run.state), Some(StepState::Passed), "{step}");
    }
    assert_eq!(world.provider.count("changes.get"), 1);
    assert_eq!(world.provider.count("tracker.comment"), 1);
    assert_eq!(ran(&world, "code.") + ran(&world, "pipelines."), 0);
}

#[tokio::test(flavor = "multi_thread")]
async fn the_invoice_and_order_example_loads_with_its_declared_criterion() {
    let world = World::new();
    let kernel = world.start().await;
    let script = kernel.shared.script("invoice-match").expect("the example compiles");
    assert_eq!(script.target, json!({ "workflow": "nf-pedido" }));
    let steps = script.described["workflows"][0]["steps"].as_array().cloned().unwrap_or_default();
    let kinds: Vec<&str> = steps.iter().filter_map(|step| step["step"].as_str()).collect();
    assert_eq!(kinds, ["wait", "act", "verify", "assess", "act", "deliver"]);
    assert_eq!(steps[2]["options"]["onFail"], "hold");
}
