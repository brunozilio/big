use std::collections::BTreeSet;

use serde_json::Value;

use super::event::WorkflowEvent;
use super::item::{ChildLink, EffectEntry, Ending, HumanAnswer, Impediment, Observed, StepRun, StepState, WorkItem};
use crate::effect::{Effect, EffectOutcome, EffectRequest, IdempotencyKey};

pub fn merge_patch(target: &mut Value, patch: &Value) {
    let Value::Object(fields) = patch else {
        *target = patch.clone();
        return;
    };
    if !target.is_object() {
        *target = Value::Object(serde_json::Map::new());
    }
    let Value::Object(kept) = target else { return };
    for (key, value) in fields {
        if value.is_null() {
            kept.remove(key);
        } else {
            merge_patch(kept.entry(key.clone()).or_insert(Value::Null), value);
        }
    }
}

pub fn replay<'a>(events: impl IntoIterator<Item = &'a WorkflowEvent>) -> WorkItem {
    events.into_iter().fold(WorkItem::default(), reduce)
}

pub fn reduce(mut item: WorkItem, event: &WorkflowEvent) -> WorkItem {
    item.version += 1;
    match event {
        WorkflowEvent::ItemCreated { id, workflow, parent, input } => {
            item.id = id.clone();
            item.workflow.clone_from(workflow);
            item.parent.clone_from(parent);
            item.input = input.clone();
        }
        WorkflowEvent::StepStarted { step, branch } => start(&mut item, step, branch.as_deref()),
        WorkflowEvent::EffectRequested(request) => request_effect(&mut item, request),
        WorkflowEvent::EffectResolved { key, outcome } => resolve(&mut item, key, outcome),
        WorkflowEvent::EffectProgress { key, data } => {
            if let Some(entry) = item.effects.get_mut(&key.to_string()) {
                merge_patch(&mut entry.progress, data);
            }
        }
        WorkflowEvent::Signal { kind, branch, step: Some(step), data, .. } if kind == STEP_RESET => {
            reset(&mut item, step, branch, data);
        }
        WorkflowEvent::Signal { .. } => {}
        WorkflowEvent::StepFinished { step, branch, state, output } => {
            let run = run_mut(&mut item, step, branch.as_deref());
            run.state = *state;
            run.output = output.clone();
        }
        WorkflowEvent::StatusSet(status) => item.status = Some(status.clone()),
        WorkflowEvent::ImpedimentSet { on, reason } => {
            item.impediment = on.then(|| Impediment { reason: reason.clone() });
        }
        WorkflowEvent::HumanInput { step, branch, action, payload } => {
            answer(
                &mut item,
                HumanAnswer {
                    step: step.clone(),
                    branch: branch.clone(),
                    action: action.clone(),
                    payload: payload.clone(),
                },
            );
        }
        WorkflowEvent::ChildStarted { child, workflow } => {
            item.children.push(ChildLink {
                id: child.clone(),
                workflow: workflow.clone(),
                finished: false,
                output: Value::Null,
            });
        }
        WorkflowEvent::ChildFinished { child, output } => finish_child(&mut item, child, output),
        WorkflowEvent::TimerSet { id, at } => {
            item.timers.insert(id.clone(), *at);
        }
        WorkflowEvent::TimerFired { id } => {
            item.timers.remove(id);
        }
        WorkflowEvent::Rewound { to, .. } => rewind(&mut item, to),
        WorkflowEvent::Cancelled { reason } => item.ending = Some(Ending::Cancelled { reason: reason.clone() }),
        WorkflowEvent::Finished { output } => item.ending = Some(Ending::Finished { output: output.clone() }),
        WorkflowEvent::ExternalObserved(seen) => observe(&mut item, seen),
        WorkflowEvent::Reconciled { .. } => {}
        WorkflowEvent::Named { title } => item.name = Some(title.clone()),
        WorkflowEvent::Branched { name } => {
            item.branch.get_or_insert_with(|| name.clone());
        }
        WorkflowEvent::Recapped { recap } => item.recap = Some(recap.clone()),
        WorkflowEvent::Linked { item: member } => {
            item.released.retain(|known| known != member);
            if !item.links.contains(member) {
                item.links.push(member.clone());
            }
        }
        WorkflowEvent::Unlinked { item: member } => {
            item.links.retain(|known| known != member);
            if !item.released.contains(member) {
                item.released.push(member.clone());
            }
        }
        WorkflowEvent::Accepting { on } => item.accepting = *on,
        WorkflowEvent::BaseChosen { repository, base } => choose_base(&mut item, repository, base),
    }
    item
}

fn choose_base(item: &mut WorkItem, repository: &str, base: &str) {
    if !item.input["git"].is_object() {
        item.input["git"] = Value::Object(serde_json::Map::new());
    }
    if !item.input["git"][repository].is_object() {
        item.input["git"][repository] = Value::Object(serde_json::Map::new());
    }
    item.input["git"][repository]["base"] = Value::String(base.to_owned());
}

fn run_mut<'a>(item: &'a mut WorkItem, id: &str, branch: Option<&str>) -> &'a mut StepRun {
    let at = match item.steps.iter().position(|run| run.is(id, branch)) {
        Some(at) => at,
        None => {
            item.steps.push(StepRun {
                id: id.to_owned(),
                branch: branch.map(str::to_owned),
                view: Value::Null,
                ..StepRun::default()
            });
            item.steps.len() - 1
        }
    };
    &mut item.steps[at]
}

const STEP_RESET: &str = "step.reset";

fn within(run: &StepRun, step: &str, branch: &str) -> bool {
    let path = run.id == step || run.id.strip_prefix(step).is_some_and(|rest| rest.starts_with('/'));
    let lane = run
        .branch
        .as_deref()
        .is_none_or(|own| own == branch || own.strip_prefix(branch).is_some_and(|rest| rest.starts_with('/')));
    path && lane
}

fn reset(item: &mut WorkItem, step: &str, branch: &str, data: &Value) {
    let state = match data["state"].as_str() {
        Some("skipped") => StepState::Skipped,
        _ => StepState::Pending,
    };
    for run in item.steps.iter_mut().filter(|run| within(run, step, branch)) {
        if matches!(run.state, StepState::Running | StepState::WaitingHuman) || state == StepState::Pending {
            run.state = state;
        }
    }
}

fn start(item: &mut WorkItem, id: &str, branch: Option<&str>) {
    let run = run_mut(item, id, branch);
    run.state = StepState::Running;
    run.attempt += 1;
    run.output = Value::Null;
}

fn request_effect(item: &mut WorkItem, request: &EffectRequest) {
    if let Effect::WaitHuman { view } = &request.effect {
        let run = run_mut(item, &request.key.step, request.key.branch.as_deref());
        run.state = StepState::WaitingHuman;
        run.view = view.clone();
    }
    item.effects.insert(
        request.key.to_string(),
        EffectEntry { request: request.clone(), outcome: None, progress: Value::Null },
    );
}

fn resolve(item: &mut WorkItem, key: &IdempotencyKey, outcome: &EffectOutcome) {
    if let Some(entry) = item.effects.get_mut(&key.to_string()) {
        entry.outcome = Some(outcome.clone());
    }
    resume(item, &key.step, key.branch.as_deref());
}

fn answer(item: &mut WorkItem, answer: HumanAnswer) {
    resume(item, &answer.step, answer.branch.as_deref());
    item.inputs.push(answer);
}

fn resume(item: &mut WorkItem, id: &str, branch: Option<&str>) {
    if let Some(run) = item.steps.iter_mut().find(|run| run.is(id, branch))
        && run.state == StepState::WaitingHuman
    {
        run.state = StepState::Running;
    }
}

fn observe(item: &mut WorkItem, seen: &Observed) {
    let slot = seen.slot();
    if item.observed.get(&slot).is_none_or(|known| known.at <= seen.at) {
        item.observed.insert(slot, seen.clone());
    }
}

fn finish_child(item: &mut WorkItem, child: &super::item::ItemId, output: &Value) {
    if let Some(link) = item.children.iter_mut().find(|link| &link.id == child) {
        link.finished = true;
        link.output = output.clone();
    }
}

fn rewind(item: &mut WorkItem, to: &str) {
    let Some(first) = item.steps.iter().position(|run| run.id == to) else {
        return;
    };
    let dropped: BTreeSet<String> = item.steps[first..].iter().map(|run| run.id.clone()).collect();
    let tail = item.steps.split_off(first);
    item.steps.extend(tail.into_iter().filter(|run| run.id == to).map(|run| StepRun {
        state: StepState::Pending,
        view: Value::Null,
        output: Value::Null,
        ..run
    }));
    item.effects.retain(|_, entry| !dropped.contains(&entry.request.key.step));
    item.inputs.retain(|input| !dropped.contains(&input.step));
    item.ending = None;
    item.impediment = None;
}
