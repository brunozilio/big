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
