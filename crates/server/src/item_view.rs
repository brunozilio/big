use connectors::ProviderDescriptor;
use domain::board::Group;
use domain::workflow::{StepState, WorkItem};
use orchestrator::{ItemSummary, NEEDS_CONNECTION, NEEDS_RECONNECT, StepSummary};
use serde_json::{Value, json};
use store::ConnectionRecord;

use crate::marks::{iso, mark, mark_of, offering, reference, serving};

pub struct Known<'a> {
    pub connections: &'a [ConnectionRecord],
    pub stale: &'a [String],
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
    if let Some(needs) = meta["needs"].as_str() {
        value["needs"] = json!(needs);
    }
    let serves = meta["needs"].as_str().and_then(|needs| serving(known.connections, needs));
    if let Some(record) = serves {
        let mark = mark_of(known.descriptors, &record.provider);
        value["logo"] = mark["logo"].clone();
        value["provider"] = mark["name"].clone();
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

fn listed_sources(item: Option<&WorkItem>) -> Vec<Value> {
    let Some(item) = item else { return Vec::new() };
    if let Some(found) = item.input["sources"].as_array() {
        return found.clone();
    }
    let legacy = text(&item.input["issue"]).map(|issue| {
        json!({ "kind": "issue", "connection": item.input["connection"].clone(), "subject": issue })
    });
    legacy.into_iter().collect()
}

fn source_json(known: &Known<'_>, source: &Value) -> Value {
    let subject = text(&source["subject"]).unwrap_or_default();
    let connection = source["connection"].as_str();
    let record = connection.and_then(|id| known.connections.iter().find(|record| record.id == id));
    let mut value = match record {
        Some(record) => reference(known.descriptors, record, &subject, None),
        None => json!({ "provider": null, "name": null, "logo": null, "connection": connection, "key": subject, "url": null }),
    };
    value["kind"] = source["kind"].clone();
    value["repository"] = json!(source["repository"].as_str());
    value
}

fn sources(known: &Known<'_>, item: Option<&WorkItem>) -> Vec<Value> {
    listed_sources(item).iter().map(|source| source_json(known, source)).collect()
}

fn repository_name(item: &WorkItem, id: &str) -> String {
    let repositories = item.input["application"]["repositories"].as_array().map(Vec::as_slice).unwrap_or_default();
    let found = repositories.iter().find(|repository| repository["id"] == id);
    let named = found.and_then(|repository| text(&repository["code"]["repo"]).or_else(|| text(&repository["directory"])));
    named.unwrap_or_else(|| id.to_owned())
}

fn git(item: Option<&WorkItem>) -> Vec<Value> {
    let Some(item) = item else { return Vec::new() };
    let wanted = item.input["git"].as_object().into_iter().flatten();
    wanted
        .map(|(id, branches)| {
            json!({
                "repository": id, "name": repository_name(item, id),
                "base": text(&branches["base"]), "head": text(&branches["head"]),
            })
        })
        .collect()
}

fn origin_change(known: &Known<'_>, item: Option<&WorkItem>) -> Value {
    let listed = listed_sources(item);
    let Some(pull) = listed.iter().find(|source| source["kind"] == "pull_request") else { return Value::Null };
    let key = text(&pull["subject"]).unwrap_or_default();
    let chosen = pull["connection"].as_str().and_then(|id| known.connections.iter().find(|record| record.id == id));
    let Some(record) = chosen.or_else(|| serving(known.connections, "code")) else { return Value::Null };
    let mut value = reference(known.descriptors, record, &key, None);
    value["checks"] = Value::Null;
    value["merged"] = json!(false);
    value
}

fn change(known: &Known<'_>, summary: &ItemSummary, item: Option<&WorkItem>) -> Value {
    let found = outputs(item);
    let Some(opened) = found.iter().rev().find(|output| text(&output["pr"]).is_some()) else {
        return origin_change(known, item);
    };
    let Some(record) = serving(known.connections, "code") else { return Value::Null };
    let key = text(&opened["pr"]).unwrap_or_default();
    let merged = found.iter().any(|output| text(&output["pr"]).as_deref() == Some(&key) && output["merged"] == true);
    let mut value = reference(known.descriptors, record, &key, opened["url"].as_str());
    value["checks"] = item.map_or(Value::Null, |item| checks(known, summary, item));
    value["merged"] = json!(merged);
    value
}

fn reconnect(known: &Known<'_>, reason: &str) -> Option<Value> {
    reason.strip_prefix(NEEDS_RECONNECT)?;
    let lost = known.connections.iter().find(|record| known.stale.contains(&record.id))?;
    Some(mark_of(known.descriptors, &lost.provider))
}

fn impediment(known: &Known<'_>, summary: &ItemSummary) -> Value {
    let Some(found) = &summary.impediment else { return Value::Null };
    let reason = found.reason.clone().unwrap_or_else(|| "o item está impedido".to_owned());
    let connect = reason
        .strip_prefix(NEEDS_CONNECTION)
        .and_then(|wanted| offering(known.descriptors, wanted.trim()))
        .map(mark)
        .or_else(|| reconnect(known, &reason))
        .unwrap_or(Value::Null);
    json!({ "reason": headline(&reason, &connect), "detail": reason, "connect": connect })
}

const MOST_HEADLINE: usize = 48;

pub fn headline(reason: &str, connect: &Value) -> String {
    let name = connect["name"].as_str();
    if reason.starts_with(NEEDS_RECONNECT) {
        return name.map_or_else(|| "Reconecte a conta".to_owned(), |name| format!("Reconecte o {name}"));
    }
    if reason.starts_with(NEEDS_CONNECTION) {
        return name.map_or_else(|| "Conecte uma conta".to_owned(), |name| format!("Conecte o {name}"));
    }
    if reason.starts_with("o código do workflow mudou") {
        return "O workflow mudou".to_owned();
    }
    let head = reason.split_once(": ").map_or(reason, |(head, _)| head).trim();
    let mut short: String = head.chars().take(MOST_HEADLINE).collect();
    if head.chars().count() > MOST_HEADLINE {
        short.push('…');
    }
    let mut letters = short.chars();
    letters.next().map(|first| first.to_uppercase().chain(letters).collect()).unwrap_or_default()
}

fn short_reason(known: &Known<'_>, reason: &str) -> String {
    headline(reason, &reconnect(known, reason).unwrap_or(Value::Null))
}

fn attention(summary: &ItemSummary) -> Value {
    let waiting = summary.steps.iter().find(|step| step.state == StepState::WaitingHuman).map(|step| step.id.clone());
    let at = summary.current.clone().or_else(|| waiting.clone());
    if summary.asks > 0 && summary.plans == summary.asks {
        let action = at.map(|step| json!({ "label": "Revisar plano", "icon": "sPlan", "step": step }));
        let badge = json!({ "text": "Plano pronto", "tone": "info", "icon": "sPlan" });
        return json!({ "badge": badge, "diff": null, "action": action });
    }
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
        let text =
            found.reason.as_deref().map_or_else(|| "Impedido".to_owned(), |reason| headline(reason, &Value::Null));
        return json!({ "badge": { "text": text, "tone": "danger" }, "diff": null, "action": null });
    }
    Value::Null
}

fn activity(known: &Known<'_>, summary: &ItemSummary) -> Value {
    if let Some(found) = &summary.impediment {
        let since = iso(summary.updated_at);
        let text = found.reason.as_deref().map_or_else(|| "Impedido".to_owned(), |reason| short_reason(known, reason));
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
        "sources": sources(known, item),
        "git": git(item),
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

#[cfg(test)]
mod tests {
    use super::*;

    fn record(id: &str, provider: &str) -> ConnectionRecord {
        ConnectionRecord {
            id: id.into(),
            provider: provider.into(),
            label: provider.into(),
            account: None,
            capabilities: vec![],
            settings: Value::Null,
        }
    }

    #[test]
    fn an_expired_login_offers_to_reconnect_the_account_that_lost_it() {
        let connections = [record("c-gh", "github"), record("c-jira", "jira")];
        let stale = ["c-jira".to_owned()];
        let known = Known { connections: &connections, stale: &stale, descriptors: &[], labels: &Value::Null };
        let reason = format!("{NEEDS_RECONNECT}refresh_token is invalid");
        assert_eq!(reconnect(&known, &reason).map(|found| found["provider"].clone()), Some(json!("jira")));
        assert_eq!(reconnect(&known, "outra falha"), None);
    }

    #[test]
    fn an_impediment_reads_as_a_few_words_and_keeps_the_detail_for_the_step() {
        let jira = json!({ "provider": "jira", "name": "Jira" });
        let expired = format!("{NEEDS_RECONNECT}connector: Auth: the access could not be renewed");
        assert_eq!(headline(&expired, &jira), "Reconecte o Jira");
        assert_eq!(
            headline("Develop falhou: a pasta de trabalho não é um repositório git", &Value::Null),
            "Develop falhou"
        );
        assert_eq!(headline("o código do workflow mudou e não reproduz o histórico", &Value::Null), "O workflow mudou");
        assert!(headline(&"x".repeat(200), &Value::Null).chars().count() <= MOST_HEADLINE + 1);
    }
}
