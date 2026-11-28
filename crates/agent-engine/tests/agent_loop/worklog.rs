use std::sync::Arc;

use agent_engine::model::scripted::ScriptedModel;
use agent_engine::tools::Tool;
use agent_engine::tools::skill::discover;
use agent_engine::transcript::{Block, Message};
use domain::workflow::ledger::{Ask, CostView, HostLedger, ItemView, Ledger, Window, cost_view, item_view};
use domain::workflow::{ItemId, StepState, WorkflowEvent, replay};
use domain::{AskKind, EngineEvent, ModelOption, PromptRequest};
use serde_json::json;

use crate::{Rig, tool_use};

struct Kept;

fn yesterday() -> Vec<(u64, i64, WorkflowEvent)> {
    let created = WorkflowEvent::ItemCreated {
        id: ItemId::from("i-ana"),
        workflow: "feature".into(),
        parent: None,
        input: json!({ "title": "Corrigir o login", "owner": "u-ana" }),
    };
    let plan = WorkflowEvent::StepFinished {
        step: "plan".into(),
        branch: None,
        state: StepState::Passed,
        output: json!(null),
    };
    let develop = WorkflowEvent::StepStarted { step: "develop".into(), branch: None };
    let plan_started = WorkflowEvent::StepStarted { step: "plan".into(), branch: None };
    vec![(1, 5, created), (2, 6, plan_started), (3, 150, plan), (4, 160, develop)]
}

impl Ledger for Kept {
    fn items(&self, asker: &str, ask: &Ask) -> Result<Vec<ItemView>, String> {
        if ask.person.as_deref().is_some_and(|person| person != asker) {
            return Err(format!("{asker} may read only their own work; reading what bia did needs the admin grant"));
        }
        let events = yesterday();
        let all: Vec<WorkflowEvent> = events.iter().map(|(_, _, event)| event.clone()).collect();
        Ok(vec![item_view(&replay(&all), asker, &events, Window { since_ms: 100, until_ms: 200 }, &[])])
    }
    fn item(&self, _asker: &str, id: &str, _ask: &Ask) -> Result<ItemView, String> {
        Err(format!("there is no work item {id} you can read"))
    }
    fn cost(&self, _asker: &str, ask: &Ask) -> Result<CostView, String> {
        Ok(cost_view(ask.person.clone(), ask.window(), &[], &|id| id.to_owned()))
    }
}

fn results(history: &[Message]) -> Vec<(String, bool)> {
    history
        .iter()
        .flat_map(|message| message.blocks.iter())
        .filter_map(|block| match block {
            Block::ToolResult { content, is_error, .. } => Some((content.clone(), is_error.to_owned())),
            _ => None,
        })
        .collect()
}

fn rig() -> Rig {
    let mut rig = Rig::new(domain::Reviewer::Human);
    rig.workspace.services.ledger = Some(HostLedger(Arc::new(Kept)));
    rig.workspace.owner = Some("u-ana".into());
    rig
}

fn skill_tool(rig: &Rig) -> Box<dyn Tool> {
    let request = PromptRequest::plain(rig.dir.path().to_path_buf(), "hi", ModelOption::new("m", "M"));
    agent_engine::tools::skill::tool(discover(request.folder(), None)).expect("Skill ships")
}

fn asked_permission(rig: &Rig) -> bool {
    rig.events()
        .into_iter()
        .any(|event| matches!(event, EngineEvent::Asked(ask) if matches!(ask.kind, AskKind::Permission { .. })))
}

#[test]
fn the_standup_skill_answers_from_the_work_log_without_asking() {
    let rig = rig();
    let model = ScriptedModel::new([
        ScriptedModel::reply(vec![tool_use("s", "Skill", json!({ "skill": "standup" }))]),
        ScriptedModel::reply(vec![tool_use("w", "Worklog", json!({ "action": "items", "days": 1 }))]),
        ScriptedModel::reply(vec![Block::Text("Ok.".into())]),
    ]);
    let mut history = vec![Message::user("o que eu fiz ontem?")];
    rig.run_with(&model, &mut history, vec![skill_tool(&rig)]).expect("run");
    let found = results(&history);
    assert!(found[0].0.contains("Worklog"), "{found:?}");
    let (log, failed) = &found[1];
    assert!(!failed, "{log}");
    assert!(log.contains("Corrigir o login [i-ana]"), "{log}");
    assert!(log.contains("done: ") && log.contains("step plan passed"), "{log}");
    assert!(log.contains("next: develop running"), "{log}");
    assert!(!asked_permission(&rig), "a read of the work log never asks");
}

#[test]
fn reading_someone_else_s_work_comes_back_refused() {
    let rig = rig();
    let model = ScriptedModel::new([
        ScriptedModel::reply(vec![tool_use("w", "Worklog", json!({ "action": "items", "person": "u-bia" }))]),
        ScriptedModel::reply(vec![Block::Text("Ok.".into())]),
    ]);
    let mut history = vec![Message::user("o que a bia fez ontem?")];
    rig.run(&model, &mut history).expect("run");
    let (text, failed) = &results(&history)[0];
    assert!(failed, "{text}");
    assert!(text.contains("admin grant"), "{text}");
}
