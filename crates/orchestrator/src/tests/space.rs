use domain::workflow::StepState;
use serde_json::json;

use super::*;

const DRAFT_ONLY: &str = r#"
import { workflow } from "sdk";
import { draft } from "sdk/steps/general.js";
export default workflow({ name: "rascunho", steps: [draft({ agent: "worker" })] });
"#;

#[tokio::test(flavor = "multi_thread")]
async fn an_agent_in_a_space_gets_its_instructions_and_required_skills_with_no_repository() {
    let world = World::new();
    world.workflow("rascunho", DRAFT_ONLY);
    let space = store::Application {
        id: "rh".into(),
        name: "RH".into(),
        instructions: "Responda candidatos em até dois dias.".into(),
        skills: vec!["triagem".into(), " ".into()],
        ..store::Application::default()
    };
    world.store().save_application(&space).expect("space");
    let kernel = world.start().await;
    let input = json!({ "title": "Responder Ana", "application": { "id": "rh", "name": "RH", "bindings": {}, "repositories": [] } });
    let id = kernel.create_item("rascunho", input).expect("item");
    let item = ended(&kernel, &id).await;
    assert_eq!(item.step("draft", Some("main")).map(|run| run.state), Some(StepState::Passed));
    let asked = world.engine.requests()[0].clone();
    let system = asked.session.append_system.unwrap_or_default();
    assert!(system.contains("# Instructions for the work in RH\nResponda candidatos em até dois dias."), "{system}");
    assert_eq!(asked.session.skills, ["triagem"]);
}

#[tokio::test(flavor = "multi_thread")]
async fn an_agent_outside_any_space_runs_as_before() {
    let world = World::new();
    world.workflow("rascunho", DRAFT_ONLY);
    let kernel = world.start().await;
    let id = kernel.create_item("rascunho", json!({ "title": "Responder Ana" })).expect("item");
    ended(&kernel, &id).await;
    let asked = world.engine.requests()[0].clone();
    assert!(asked.session.skills.is_empty());
    assert!(!asked.session.append_system.unwrap_or_default().contains("Instructions for the work"));
}

const LOOKUP: &str = r#"
import { workflow } from "sdk";
import { act } from "sdk/steps/general.js";
export default workflow({ name: "consulta", steps: [act({ capability: "changes", action: "get", args: { change: "GMUD-1" } })] });
"#;

async fn looked_up(bound_now: bool) -> WorkItem {
    let world = World::new();
    world.workflow("consulta", LOOKUP);
    world.provider.always("changes.get", json!({ "id": "GMUD-1" }));
    let unsigned = store::ConnectionRecord {
        id: "c2".into(),
        provider: "fake".into(),
        label: "Sem login".into(),
        account: None,
        capabilities: vec!["changes".into()],
        settings: json!({}),
    };
    let binding = store::Bindings::from([("changes".to_owned(), store::Binding::new("c2", "OPS"))]);
    let live = if bound_now { binding.clone() } else { store::Bindings::new() };
    let space =
        store::Application { id: "ops".into(), name: "Ops".into(), bindings: live, ..store::Application::default() };
    world.store().save_application(&space).expect("space");
    let kernel = world.start().await;
    world.store().save_connection(&unsigned).expect("listed after the first one");
    let snapshot = json!({ "id": "ops", "name": "Ops", "bindings": binding, "repositories": [] });
    let id = kernel.create_item("consulta", json!({ "title": "GMUD-1", "application": snapshot })).expect("item");
    until(&kernel, &id, |item| item.is_ended() || item.impediment.is_some()).await
}

#[tokio::test(flavor = "multi_thread")]
async fn a_call_goes_through_the_connection_the_live_space_binds() {
    let item = looked_up(true).await;
    assert!(item.impediment.is_some(), "the bound connection has no sign in: {:?}", item.steps);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_binding_removed_from_the_space_no_longer_routes_the_call() {
    let item = looked_up(false).await;
    assert!(item.impediment.is_none(), "{:?}", item.impediment);
    assert_eq!(item.step("act", Some("main")).map(|run| run.state), Some(StepState::Passed));
}
