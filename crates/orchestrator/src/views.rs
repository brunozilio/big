use domain::board::{Group, group_of};
use domain::effect::Effect;
use domain::workflow::{Ending, Impediment, ItemId, StepState, WorkItem, WorkflowEvent};
use serde::Serialize;
use serde_json::{Map, Value, json};
use sync::ReconcilePolicy;

use crate::git;
use crate::ledger::Ledger;
use crate::protocol::{AGENT, WAIT_HUMAN, kind_of};

pub fn item_json(item: &WorkItem) -> Value {
    let input = &item.input;
    let text = |name: &str| input.get(name).and_then(Value::as_str).filter(|text| !text.is_empty());
    let title = text("title").or_else(|| text("text").and_then(|body| body.lines().next())).unwrap_or_default();
    let mut fields: Map<String, Value> = input.as_object().cloned().unwrap_or_default();
    fields.insert("id".into(), json!(item.id));
    fields.insert("workflow".into(), json!(item.workflow));
    fields.insert("title".into(), json!(title));
    fields
        .insert("labels".into(), input.get("labels").filter(|labels| labels.is_array()).cloned().unwrap_or(json!([])));
    fields.insert("branch".into(), json!(git::branch_name(item.id.as_str())));
    fields.insert("issue".into(), json!(text("issue")));
    fields.insert("description".into(), json!(text("description").or_else(|| text("text"))));
    fields.insert("input".into(), input.clone());
    Value::Object(fields)
}

pub fn linked_issue(item: &WorkItem) -> Option<String> {
    sync::desired_of(item, ReconcilePolicy::default()).issue.map(|issue| issue.issue)
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct StepSummary {
    pub id: String,
    pub branch: Option<String>,
    pub kind: Option<String>,
    pub state: StepState,
    pub attempts: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ItemSummary {
    pub id: ItemId,
    pub title: String,
    pub workflow: String,
    pub parent: Option<ItemId>,
    pub group: Group,
    pub status: Option<String>,
    pub issue: Option<String>,
    pub branch: String,
    pub impediment: Option<Impediment>,
    pub asks: usize,
    pub steps: Vec<StepSummary>,
    pub current: Option<String>,
    pub live: bool,
    pub ending: Option<Ending>,
    pub created_at: i64,
    pub updated_at: i64,
    pub ended_at: Option<i64>,
}

pub fn summary(
    item: &WorkItem,
    ledger: &Ledger,
    times: &[(i64, bool)],
    asks: usize,
    declared: &[String],
) -> ItemSummary {
    let mut steps: Vec<StepSummary> = item
        .steps
        .iter()
        .map(|run| StepSummary {
            id: run.id.clone(),
            branch: run.branch.clone(),
            kind: ledger.step_type(&run.id),
            state: run.state,
            attempts: run.attempt,
        })
        .collect();
    for id in declared.iter().filter(|id| !item.steps.iter().any(|run| &run.id == *id)) {
        steps.push(StepSummary { id: id.clone(), branch: None, kind: None, state: StepState::Pending, attempts: 0 });
    }
    let active = |state: StepState| matches!(state, StepState::Running | StepState::WaitingHuman);
    let current = item.steps.iter().rev().find(|run| active(run.state)).map(|run| run.id.clone());
    let live = ledger.open().any(|request| kind_of(&request.effect) == AGENT);
    ItemSummary {
        id: item.id.clone(),
        title: item_json(item)["title"].as_str().unwrap_or_default().to_owned(),
        workflow: item.workflow.clone(),
        parent: item.parent.clone(),
        group: group_of(item, asks > 0),
        status: item.status.clone(),
        issue: linked_issue(item),
        branch: git::branch_name(item.id.as_str()),
        impediment: item.impediment.clone(),
        asks,
        steps,
        current,
        live,
        ending: item.ending.clone(),
        created_at: times.first().map_or(0, |(at, _)| *at),
        updated_at: times.last().map_or(0, |(at, _)| *at),
        ended_at: if item.is_ended() { times.iter().rev().find(|(_, ends)| *ends).map(|(at, _)| *at) } else { None },
    }
}

pub fn ends(event: &WorkflowEvent) -> bool {
    matches!(event, WorkflowEvent::Finished { .. } | WorkflowEvent::Cancelled { .. })
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Attempt {
    pub n: u32,
    pub state: StepState,
    pub started_at: i64,
    pub ended_at: Option<i64>,
    pub data: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct StepView {
    pub item: ItemId,
    pub step: String,
    pub kind: Option<String>,
    pub attempt: u32,
    pub attempts: Vec<Attempt>,
    pub session: Option<String>,
    pub blocks: Value,
}

fn finished_state(outcome: &str) -> StepState {
    match outcome {
        "failed" => StepState::Failed,
        "cancelled" => StepState::Skipped,
        _ => StepState::Passed,
    }
}

pub fn attempts(ledger: &Ledger, step: &str) -> Vec<Attempt> {
    let mut found: Vec<Attempt> = Vec::new();
    for signal in ledger.signals.values().filter(|signal| signal.step.as_deref() == Some(step)) {
        match signal.kind.as_str() {
            "step.started" => found.push(Attempt {
                n: signal.attempt,
                state: StepState::Running,
                started_at: signal.at,
                ended_at: None,
                data: Value::Null,
            }),
            "step.finished" => {
                if let Some(open) = found.iter_mut().rev().find(|attempt| attempt.n == signal.attempt) {
                    open.state = finished_state(signal.data["outcome"].as_str().unwrap_or_default());
                    open.ended_at = Some(signal.at);
                    open.data = signal.data["data"].clone();
                }
            }
            _ => {}
        }
    }
    found
}

pub fn session_of(ledger: &Ledger, step: &str) -> Option<String> {
    ledger
        .requested
        .iter()
        .rev()
        .filter(|(_, request)| request.key.step == step && kind_of(&request.effect) == AGENT)
        .find_map(|(seq, _)| {
            let progress = ledger.progress.get(seq).and_then(|data| data["session"].as_str());
            let value = ledger.value(*seq).and_then(|value| value["session"].as_str());
            value.or(progress).map(str::to_owned)
        })
}

pub fn live_blocks(ledger: &Ledger, step: &str) -> Option<Value> {
    let open: Vec<_> = ledger.open().filter(|request| request.key.step == step).collect();
    if let Some(Effect::WaitHuman { view }) =
        open.iter().rev().map(|request| &request.effect).find(|effect| kind_of(effect) == WAIT_HUMAN)
    {
        return Some(view.clone());
    }
    let running = open.iter().rev().find(|request| kind_of(&request.effect) == AGENT)?;
    let session = ledger.progress.get(&running.key.seq).and_then(|data| data["session"].as_str())?;
    Some(json!([{ "type": "chat", "session": session }]))
}

pub fn view_state(item: &WorkItem, ledger: &Ledger, step: &str, chosen: Option<&Attempt>) -> Value {
    let run = item.steps.iter().rev().find(|run| run.id == step);
    let (state, output) = match chosen {
        Some(attempt) => (attempt.state, attempt.data.clone()),
        None => (run.map_or(StepState::Pending, |run| run.state), run.map_or(Value::Null, |run| run.output.clone())),
    };
    let mut fields: Map<String, Value> = output.as_object().cloned().unwrap_or_default();
    fields.insert("id".into(), json!(step));
    fields.insert("state".into(), serde_json::to_value(state).unwrap_or(Value::Null));
    fields
        .insert("attempt".into(), json!(chosen.map_or_else(|| run.map_or(0, |run| run.attempt), |attempt| attempt.n)));
    fields.insert("passed".into(), json!(state == StepState::Passed));
    fields.insert("failed".into(), json!(state == StepState::Failed));
    if !fields.contains_key("session")
        && let Some(session) = session_of(ledger, step)
    {
        fields.insert("session".into(), json!(session));
    }
    Value::Object(fields)
}
