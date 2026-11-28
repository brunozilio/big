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

const DRAFT_ONLY: &str = r#"
import { workflow } from "sdk";
import { draft } from "sdk/steps/general.js";
export default workflow({ name: "rascunho", steps: [draft({ agent: "worker" })] });
"#;

#[tokio::test(flavor = "multi_thread")]
async fn an_agent_with_no_repository_bound_drafts_in_the_scratch_folder_of_the_item() {
    let world = World::new();
    world.workflow("rascunho", DRAFT_ONLY);
    let kernel = world.start().await;
    let id = kernel.create_item("rascunho", json!({ "title": "Responder Ana" })).expect("item");
    let item = ended(&kernel, &id).await;
    assert_eq!(item.step("draft", Some("main")).map(|run| run.state), Some(StepState::Passed));
    let cwd = world.engine.requests()[0].session.cwd.clone();
    assert!(cwd.ends_with(std::path::Path::new("scratch").join(id.to_string())), "{}", cwd.display());
}

#[tokio::test(flavor = "multi_thread")]
async fn an_agent_whose_bound_repository_has_no_folder_still_fails_as_before() {
    let world = World::new();
    world.workflow("rascunho", DRAFT_ONLY);
    let kernel = world.start().await;
    let application = json!({ "id": "app", "name": "XPTO", "bindings": {},
        "repositories": [{ "id": "api", "name": "api" }] });
    let id =
        kernel.create_item("rascunho", json!({ "title": "Responder Ana", "application": application })).expect("item");
    let failed = |item: &WorkItem| item.step("draft", Some("main")).is_some_and(|run| run.state == StepState::Failed);
    let item = until(&kernel, &id, failed).await;
    let run = item.step("draft", Some("main")).cloned().expect("draft");
    assert_eq!(run.output["error"], "the item has no repository");
    assert_eq!(world.engine.runs(), 0);
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
