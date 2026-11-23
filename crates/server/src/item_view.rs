use connectors::ProviderDescriptor;
use domain::board::Group;
use domain::workflow::{StepState, WorkItem};
use orchestrator::{ItemSummary, NEEDS_CONNECTION, StepSummary};
use serde_json::{Value, json};
use store::ConnectionRecord;

use crate::marks::{iso, mark, mark_of, offering, reference, serving};

pub struct Known<'a> {
    pub connections: &'a [ConnectionRecord],
    pub descriptors: &'a [ProviderDescriptor],
    pub labels: &'a Value,
}

fn group(group: Group) -> &'static str {
    match group {
        Group::WithYou => "you",
        Group::InProgress => "running",
        Group::Delivered => "delivered",
    }
}

pub fn state(state: StepState) -> Value {
    serde_json::to_value(state).unwrap_or(json!("pending"))
}

fn meta<'a>(known: &'a Known<'_>, step: &StepSummary) -> &'a Value {
    let by_kind = step.kind.as_deref().map(|kind| &known.labels[kind]).filter(|found| !found.is_null());
    by_kind.unwrap_or(&known.labels[step.id.as_str()])
}

fn step_json(known: &Known<'_>, step: &StepSummary) -> Value {
    let meta = meta(known, step);
    let mut value = json!({
        "id": step.id,
        "label": meta["label"].as_str().unwrap_or(&step.id),
        "icon": meta["icon"].as_str().unwrap_or("sCode"),
        "state": state(step.state),
        "attempts": step.attempts,
        "seconds": step.seconds,
    });
    let logo = meta["needs"].as_str().and_then(|needs| serving(known.connections, needs));
    if let Some(record) = logo {
        value["logo"] = mark_of(known.descriptors, &record.provider)["logo"].clone();
    }
    value
}

fn outputs(item: Option<&WorkItem>) -> Vec<&Value> {
    item.map(|item| item.steps.iter().map(|run| &run.output).collect()).unwrap_or_default()
}

fn text(value: &Value) -> Option<String> {
    match value {
        Value::String(text) if !text.is_empty() => Some(text.clone()),
        Value::Number(number) => Some(number.to_string()),
        _ => None,
    }
}

fn issue(known: &Known<'_>, summary: &ItemSummary, item: Option<&WorkItem>) -> Value {
    let Some(key) = summary.issue.as_deref() else { return Value::Null };
    let chosen = item.and_then(|item| item.input["connection"].as_str());
    let record = chosen
        .and_then(|id| known.connections.iter().find(|record| record.id == id))
        .or_else(|| serving(known.connections, "tracker"));
    let Some(record) = record else { return Value::Null };
    let url = outputs(item).into_iter().find(|output| text(&output["issue"]).as_deref() == Some(key));
    reference(known.descriptors, record, key, url.and_then(|output| output["url"].as_str()))
}

fn checks_state(state: StepState) -> Value {
    match state {
        StepState::Running | StepState::WaitingHuman => json!("running"),
        StepState::Passed => json!("passed"),
        StepState::Failed => json!("failed"),
        StepState::Pending => json!("pending"),
        StepState::Skipped => Value::Null,
    }
}

fn checks(known: &Known<'_>, summary: &ItemSummary, item: &WorkItem) -> Value {
    let Some(opened) = item.steps.iter().find(|run| text(&run.output["pr"]).is_some()) else { return Value::Null };
    let Some(at) = summary.steps.iter().position(|step| step.id == opened.id) else { return Value::Null };
    summary.steps[at + 1..]
        .iter()
        .find(|step| meta(known, step)["needs"] == "pipelines")
        .map_or(Value::Null, |step| checks_state(step.state))
}

fn change(known: &Known<'_>, summary: &ItemSummary, item: Option<&WorkItem>) -> Value {
    let found = outputs(item);
    let Some(opened) = found.iter().rev().find(|output| text(&output["pr"]).is_some()) else { return Value::Null };
    let Some(record) = serving(known.connections, "code") else { return Value::Null };
    let key = text(&opened["pr"]).unwrap_or_default();
    let merged = found.iter().any(|output| text(&output["pr"]).as_deref() == Some(&key) && output["merged"] == true);
    let mut value = reference(known.descriptors, record, &key, opened["url"].as_str());
    value["checks"] = item.map_or(Value::Null, |item| checks(known, summary, item));
    value["merged"] = json!(merged);
    value
}

fn impediment(known: &Known<'_>, summary: &ItemSummary) -> Value {
    let Some(found) = &summary.impediment else { return Value::Null };
    let reason = found.reason.clone().unwrap_or_else(|| "o item está impedido".to_owned());
    let connect = reason
        .strip_prefix(NEEDS_CONNECTION)
        .and_then(|wanted| offering(known.descriptors, wanted.trim()))
        .map_or(Value::Null, mark);
    json!({ "reason": reason, "connect": connect })
}

fn attention(summary: &ItemSummary) -> Value {
    let waiting = summary.steps.iter().find(|step| step.state == StepState::WaitingHuman).map(|step| step.id.clone());
    let at = summary.current.clone().or_else(|| waiting.clone());
    if summary.asks > 0 {
        let action = at.map(|step| json!({ "label": "Responder", "icon": "chat", "step": step }));
        let badge = json!({ "text": "Pergunta do agente", "tone": "warning", "icon": "help" });
        return json!({ "badge": badge, "diff": null, "action": action });
    }
    if let Some(step) = waiting {
        let badge = json!({ "text": "Esperando você", "tone": "info" });
        return json!({ "badge": badge, "diff": null, "action": { "label": "Abrir", "step": step } });
    }
    if let Some(found) = &summary.impediment {
        let text = found.reason.clone().unwrap_or_else(|| "Impedido".to_owned());
        return json!({ "badge": { "text": text, "tone": "danger" }, "diff": null, "action": null });
    }
    Value::Null
}

fn activity(known: &Known<'_>, summary: &ItemSummary) -> Value {
    if let Some(found) = &summary.impediment {
        let since = iso(summary.updated_at);
        let text = found.reason.clone().unwrap_or_else(|| "Impedido".to_owned());
        return json!({ "text": text, "since": since, "tone": "danger" });
    }
    let Some(current) = summary.steps.iter().find(|step| Some(&step.id) == summary.current.as_ref()) else {
        return Value::Null;
    };
    let meta = meta(known, current);
    let text = meta["doing"].as_str().or(meta["label"].as_str()).unwrap_or(&current.id);
    json!({ "text": text, "since": iso(current.since.unwrap_or(summary.updated_at)) })
}

fn title(summary: &ItemSummary) -> String {
    let chosen = summary.title.trim();
    if !chosen.is_empty() {
        return chosen.to_owned();
    }
    summary.issue.clone().unwrap_or_else(|| "Item sem título".to_owned())
}

fn collapsed(steps: &[StepSummary]) -> Vec<StepSummary> {
    let mut kept: Vec<StepSummary> = Vec::new();
    for step in steps.iter().filter(|step| !step.id.is_empty()) {
        match kept.iter_mut().find(|known| known.id == step.id) {
            Some(known) => {
                if step.state == StepState::WaitingHuman {
                    known.state = StepState::WaitingHuman;
                }
                known.attempts = known.attempts.max(step.attempts);
            }
            None => kept.push(step.clone()),
        }
    }
    kept
}

fn shown_steps(summary: &ItemSummary) -> ItemSummary {
    let mut shown = summary.clone();
    shown.steps = collapsed(&summary.steps);
    let unnamed = summary.current.as_deref() == Some("")
        || summary.steps.iter().any(|step| step.id.is_empty() && step.state == StepState::WaitingHuman);
    if unnamed {
        let failed = shown.steps.iter().rev().find(|step| step.state == StepState::Failed).map(|step| step.id.clone());
        shown.current = failed.or_else(|| shown.current.clone().filter(|current| !current.is_empty()));
        for step in shown.steps.iter_mut().filter(|step| Some(&step.id) == shown.current.as_ref()) {
            step.state = StepState::WaitingHuman;
        }
    }
    shown
}

pub fn item(known: &Known<'_>, summary: &ItemSummary, item: Option<&WorkItem>) -> Value {
    let summary = &shown_steps(summary);
    json!({
        "id": summary.id.as_str(),
        "title": title(summary),
        "workflow": summary.workflow,
        "group": group(summary.group),
        "status": summary.status,
        "issue": issue(known, summary, item),
        "change": change(known, summary, item),
        "steps": summary.steps.iter().map(|step| step_json(known, step)).collect::<Vec<_>>(),
        "current": summary.current,
        "live": summary.live,
        "activity": activity(known, summary),
        "impediment": impediment(known, summary),
        "attention": attention(summary),
        "createdAt": iso(summary.created_at),
        "updatedAt": iso(summary.updated_at),
        "doneAt": summary.ended_at.map(iso),
    })
}
