use domain::board::{Group, group_of};
use domain::effect::Effect;
use domain::workflow::{Ending, Impediment, ItemId, StepState, WorkItem, WorkflowEvent};
use serde::Serialize;
use serde_json::{Map, Value, json};
use sync::ReconcilePolicy;

use crate::calls::calls_block;
use crate::git;
use crate::ledger::Ledger;
use crate::protocol::{AGENT, CALL, EXEC, WAIT_HUMAN, kind_of};
use crate::repos;

fn repositories(item: &WorkItem) -> Vec<String> {
    repos::of_input(&item.input, None).into_iter().map(|checkout| checkout.id).collect()
}

pub fn branch(item: &WorkItem) -> String {
    repositories(item).first().map_or_else(|| git::head_of(item, ""), |first| git::head_of(item, first))
}

fn heads(item: &WorkItem) -> Value {
    let resolved = repositories(item)
        .into_iter()
        .filter(|id| !id.is_empty())
        .map(|id| {
            let mut pair = item.input["git"][&id].as_object().cloned().unwrap_or_default();
            pair.insert("base".into(), json!(git::base_of(item, &id)));
            pair.insert("head".into(), json!(git::head_of(item, &id)));
            (id, Value::Object(pair))
        })
        .collect();
    Value::Object(resolved)
}

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
    fields.insert("branch".into(), json!(branch(item)));
    fields.insert("git".into(), heads(item));
    fields.insert("issue".into(), json!(text("issue")));
    fields.insert("description".into(), json!(text("description").or_else(|| text("text"))));
    fields.insert("input".into(), input.clone());
    Value::Object(fields)
}

fn planned_title(item: &WorkItem) -> Option<String> {
    item.steps.iter().rev().find_map(|run| {
        let title = run.output.get("plan").and(run.output.get("title")).and_then(Value::as_str)?.trim();
        (!title.is_empty()).then(|| title.to_owned())
    })
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
    pub seconds: u64,
    pub since: Option<i64>,
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
    pub plans: usize,
    pub steps: Vec<StepSummary>,
    pub current: Option<String>,
    pub live: bool,
    pub ending: Option<Ending>,
    pub created_at: i64,
    pub updated_at: i64,
    pub ended_at: Option<i64>,
}

fn merged(mut spans: Vec<(i64, i64)>) -> Vec<(i64, i64)> {
    spans.sort_unstable();
    let mut joined: Vec<(i64, i64)> = Vec::new();
    for (start, end) in spans.into_iter().filter(|(start, end)| end > start) {
        match joined.last_mut() {
            Some(last) if start <= last.1 => last.1 = last.1.max(end),
            _ => joined.push((start, end)),
        }
    }
    joined
}

fn length(spans: &[(i64, i64)]) -> i64 {
    spans.iter().map(|(start, end)| end - start).sum()
}

fn overlap(left: &[(i64, i64)], right: &[(i64, i64)]) -> i64 {
    let pairs = left.iter().flat_map(|a| right.iter().map(move |b| (a.0.max(b.0), a.1.min(b.1))));
    pairs.map(|(start, end)| (end - start).max(0)).sum()
}

fn working(ledger: &Ledger, step: &str, now: i64) -> i64 {
    let ended = |seq: &u64| ledger.resolved.get(seq).map_or(now, |resolution| resolution.at);
    let ours = |seq: &u64, kinds: &[&str]| {
        ledger
            .requested
            .get(seq)
            .is_some_and(|request| request.key.step == step && kinds.contains(&kind_of(&request.effect)))
    };
    let busy = merged(
        ledger
            .requested_at
            .iter()
            .filter(|(seq, _)| ours(seq, &[AGENT, EXEC, CALL]))
            .map(|(seq, start)| (*start, ended(seq)))
            .collect(),
    );
    let paused = merged(
        ledger
            .pauses
            .iter()
            .filter(|pause| ours(&pause.seq, &[AGENT]))
            .map(|pause| (pause.from, pause.to.unwrap_or_else(|| ended(&pause.seq))))
            .collect(),
    );
    length(&busy) - overlap(&busy, &paused)
}

fn timing(ledger: &Ledger, step: &str, now: i64) -> (u64, Option<i64>) {
    let since = attempts(ledger, step).last().map(|attempt| attempt.started_at);
    (u64::try_from(working(ledger, step, now) / 1000).unwrap_or(0), since)
}

fn place(declared: &[(String, Option<String>)], id: &str) -> usize {
    let mut path = id;
    loop {
        if let Some(at) = declared.iter().position(|(known, _)| known == path) {
            return at;
        }
        match path.rsplit_once('/') {
            Some((parent, _)) => path = parent,
            None => return declared.len(),
        }
    }
}

pub fn summary(
    item: &WorkItem,
    ledger: &Ledger,
    times: &[(i64, bool)],
    asks: usize,
    declared: &[(String, Option<String>)],
) -> ItemSummary {
    let now = crate::shared::now_ms();
    let mut steps: Vec<StepSummary> = item
        .steps
        .iter()
        .map(|run| {
            let (seconds, since) = timing(ledger, &run.id, now);
            StepSummary {
                id: run.id.clone(),
                branch: run.branch.clone(),
                kind: ledger.step_type(&run.id),
                state: run.state,
                attempts: run.attempt,
                seconds,
                since,
            }
        })
        .collect();
    for (id, kind) in declared.iter().filter(|(id, _)| !item.steps.iter().any(|run| &run.id == id)) {
        let pending = StepSummary {
            id: id.clone(),
            branch: None,
            kind: kind.clone(),
            state: StepState::Pending,
            attempts: 0,
            seconds: 0,
            since: None,
        };
        steps.push(pending);
    }
    steps.sort_by_key(|step| place(declared, &step.id));
    let active = |state: StepState| matches!(state, StepState::Running | StepState::WaitingHuman);
    let current = item.steps.iter().rev().find(|run| !run.id.is_empty() && active(run.state)).map(|run| run.id.clone());
    let stopped =
        item.impediment.as_ref().is_some_and(|found| !crate::texts::is_waiting_answer(found.reason.as_deref()));
    let live = !stopped && !item.is_ended() && ledger.open().any(|request| kind_of(&request.effect) == AGENT);
    ItemSummary {
        id: item.id.clone(),
        title: item
            .name
            .clone()
            .or_else(|| planned_title(item))
            .unwrap_or_else(|| item_json(item)["title"].as_str().unwrap_or_default().to_owned()),
        workflow: item.workflow.clone(),
        parent: item.parent.clone(),
        group: group_of(item, asks > 0),
        status: item.status.clone(),
        issue: linked_issue(item),
        branch: branch(item),
        impediment: item.impediment.clone(),
        asks,
        plans: 0,
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
    pub heads_up: Option<domain::workflow::HeadsUpNote>,
    pub blocks: Value,
}

fn finished_state(outcome: &str) -> StepState {
    match outcome {
        "failed" | "goto" => StepState::Failed,
        "cancelled" | "skipped" => StepState::Skipped,
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

pub fn session_of(ledger: &Ledger, step: &str, attempt: Option<u32>) -> Option<String> {
    ledger
        .requested
        .iter()
        .rev()
        .filter(|(_, request)| request.key.step == step && kind_of(&request.effect) == AGENT)
        .filter(|(_, request)| attempt.is_none_or(|wanted| request.key.attempt == wanted))
        .find_map(|(seq, _)| {
            let progress = ledger.progress.get(seq).and_then(|data| data["session"].as_str());
            let value = ledger.value(*seq).and_then(|value| value["session"].as_str());
            value.or(progress).map(str::to_owned)
        })
}

pub fn live_blocks(ledger: &Ledger, step: &str) -> Option<Value> {
    let open: Vec<_> = ledger.open().filter(|request| request.key.step == step).collect();
    if open.is_empty() {
        return None;
    }
    if let Some(Effect::WaitHuman { view }) =
        open.iter().rev().map(|request| &request.effect).find(|effect| kind_of(effect) == WAIT_HUMAN)
    {
        return Some(view.clone());
    }
    let Some(running) = open.iter().rev().find(|request| kind_of(&request.effect) == AGENT) else {
        return calls_block(ledger, step).map(|block| json!([block]));
    };
    let session = ledger.progress.get(&running.key.seq).and_then(|data| data["session"].as_str())?;
    let planning = match &running.effect {
        Effect::AgentRun { input, .. } => input["mode"] == "plan",
        _ => false,
    };
    Some(json!([{ "type": "chat", "session": session, "composer": planning }]))
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
        && let Some(session) = session_of(ledger, step, chosen.map(|attempt| attempt.n))
    {
        fields.insert("session".into(), json!(session));
    }
    Value::Object(fields)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(input: Value) -> WorkItem {
        WorkItem { id: "i1".into(), input, ..WorkItem::default() }
    }

    #[test]
    fn scripts_see_the_head_and_base_of_each_repository() {
        let repositories = json!([{ "id": "api" }, { "id": "web" }]);
        let found = item_json(&item(json!({
            "application": { "repositories": repositories },
            "git": { "api": { "base": "develop", "head": "feature/x" } },
        })));
        assert_eq!(found["branch"], "feature/x");
        assert_eq!(
            found["git"],
            json!({
                "api": { "base": "develop", "head": "feature/x" },
                "web": { "base": null, "head": git::branch_name("i1") },
            })
        );
    }

    fn effect(ledger: &mut Ledger, seq: u64, effect: Effect, span: (i64, Option<i64>)) {
        let key = domain::effect::IdempotencyKey::new("i1", "build", None, seq, 1);
        ledger.requested.insert(seq, domain::effect::EffectRequest::new(key, effect));
        ledger.requested_at.insert(seq, span.0);
        if let Some(at) = span.1 {
            let outcome = domain::effect::EffectOutcome::Ok { value: Value::Null };
            ledger.resolved.insert(seq, crate::ledger::Resolution { outcome, at });
        }
    }

    #[test]
    fn a_step_counts_only_the_time_spent_working_not_waiting() {
        let mut ledger = Ledger::default();
        let agent = || Effect::AgentRun { agent: "dev".into(), input: Value::Null };
        effect(&mut ledger, 1, agent(), (0, Some(60_000)));
        effect(&mut ledger, 2, Effect::WaitHuman { view: Value::Null }, (60_000, Some(3_600_000)));
        effect(&mut ledger, 3, Effect::Exec { command: "make".into(), cwd: None }, (3_600_000, Some(3_630_000)));
        effect(&mut ledger, 4, agent(), (3_610_000, Some(3_640_000)));
        effect(&mut ledger, 5, agent(), (4_000_000, None));
        assert_eq!(timing(&ledger, "build", 4_020_000).0, 120);
    }

    #[test]
    fn a_chat_waiting_for_an_approval_does_not_count_as_working() {
        let mut ledger = Ledger::default();
        let agent = || Effect::AgentRun { agent: "dev".into(), input: Value::Null };
        effect(&mut ledger, 1, agent(), (0, Some(4_000_000)));
        ledger.pauses.push(crate::ledger::Pause { seq: 1, ask: "a".into(), from: 60_000, to: Some(3_660_000) });
        effect(&mut ledger, 2, agent(), (5_000_000, None));
        ledger.pauses.push(crate::ledger::Pause { seq: 2, ask: "b".into(), from: 5_030_000, to: None });
        assert_eq!(timing(&ledger, "build", 9_000_000).0, 400 + 30);
    }

    #[test]
    fn an_item_without_repositories_keeps_the_generated_branch() {
        let found = item_json(&item(json!({ "title": "x" })));
        assert_eq!(found["branch"], git::branch_name("i1"));
        assert_eq!(found["git"], json!({}));
    }
}
