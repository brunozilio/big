use connectors::ProviderDescriptor;
use domain::Language;
use domain::board::Group;
use domain::workflow::{ItemId, Output, StepState, WorkItem};
use orchestrator::{ItemSummary, NEEDS_CONNECTION, NEEDS_RECONNECT, StepSummary};
use serde_json::{Value, json};
use store::ConnectionRecord;

use crate::marks::{iso, mark, mark_of, offering, reference, serving};

pub struct Known<'a> {
    pub connections: &'a [ConnectionRecord],
    pub stale: &'a [String],
    pub descriptors: &'a [ProviderDescriptor],
    pub described: &'a Value,
    pub output: Option<&'a Output>,
    pub language: Language,
}

fn language_of(known: &Known<'_>, item: Option<&WorkItem>) -> Language {
    item.and_then(|item| item.input["language"].as_str()).and_then(Language::named).unwrap_or(known.language)
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

pub(crate) fn meta(known: &Known<'_>, step: &StepSummary) -> Value {
    let labels = &known.described["labels"];
    let by_kind = step.kind.as_deref().map(|kind| &labels[kind]).filter(|found| !found.is_null());
    let mut found = by_kind.unwrap_or(&labels[step.id.as_str()]).clone();
    if let Some(own) = known.described["nodes"][step.id.as_str()].as_object() {
        if !found.is_object() {
            found = json!({});
        }
        for (key, value) in own.iter().filter(|(_, value)| !value.is_null()) {
            found[key] = value.clone();
        }
    }
    found
}

fn parent_of(id: &str) -> Option<&str> {
    id.rsplit_once('/').map(|(parent, _)| parent)
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
    if let Some(parent) = parent_of(&step.id) {
        value["parent"] = json!(parent);
        value["depth"] = json!(step.id.matches('/').count());
    }
    if let Some(requires) = meta["requires"].as_str() {
        value["requires"] = json!(requires);
    }
    let serves = meta["requires"].as_str().and_then(|requires| serving(known.connections, requires));
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

pub(crate) fn listed_sources(item: Option<&WorkItem>) -> Vec<Value> {
    let Some(item) = item else { return Vec::new() };
    if let Some(found) = item.input["sources"].as_array() {
        return found.clone();
    }
    let legacy = text(&item.input["issue"])
        .map(|issue| json!({ "kind": "issue", "connection": item.input["connection"].clone(), "subject": issue }));
    legacy.into_iter().collect()
}

fn source_json(known: &Known<'_>, source: &Value) -> Value {
    let subject = text(&source["subject"]).unwrap_or_default();
    let connection = source["connection"].as_str();
    let record = connection.and_then(|id| known.connections.iter().find(|record| record.id == id));
    let mut value = match record {
        Some(record) => reference(known.descriptors, record, &subject, None),
        None => {
            json!({ "provider": null, "name": null, "logo": null, "connection": connection, "key": subject, "url": null })
        }
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
    let named = found.and_then(|repository| {
        text(&repository["bindings"][connectors::contracts::CODE]["value"])
            .or_else(|| text(&repository["code"]["repo"]))
            .or_else(|| text(&repository["directory"]))
    });
    named.unwrap_or_else(|| id.to_owned())
}

fn git(item: Option<&WorkItem>) -> Vec<Value> {
    let Some(item) = item else { return Vec::new() };
    orchestrator::wanted_refs(item)
        .into_iter()
        .map(|wanted| {
            json!({
                "repository": wanted.repository, "name": repository_name(item, &wanted.repository),
                "base": wanted.base, "head": wanted.head,
            })
        })
        .collect()
}

fn reconnect(known: &Known<'_>, reason: &str) -> Option<Value> {
    reason.strip_prefix(NEEDS_RECONNECT)?;
    let lost = known.connections.iter().find(|record| known.stale.contains(&record.id))?;
    Some(mark_of(known.descriptors, &lost.provider))
}

fn missing_base(item: Option<&WorkItem>, reason: &str, language: Language) -> Option<Value> {
    let (repository, base) = orchestrator::missing_base(reason)?;
    let name = item.map_or_else(|| repository.clone(), |item| repository_name(item, &repository));
    let headline = language.pick(
        format!("The base branch {base} does not exist in {name}"),
        format!("La rama base {base} no existe en {name}"),
        format!("A branch base {base} não existe em {name}"),
    );
    let detail = language.pick(
        "Pick another base branch or add a new one.".to_owned(),
        "Elige otra rama base o agrega una nueva.".to_owned(),
        "Escolha outra branch base ou adicione uma nova.".to_owned(),
    );
    let base_said = key("base_missing_in", json!({ "base": base, "name": name }));
    let shown = json!({ "repository": repository, "name": name, "missing": base });
    Some(json!({ "reason": headline, "detail": detail, "connect": null, "said": base_said, "base": shown }))
}

fn impediment(known: &Known<'_>, summary: &ItemSummary, item: Option<&WorkItem>, language: Language) -> Value {
    let Some(found) = &summary.impediment else { return Value::Null };
    let reason = found.reason.clone().unwrap_or_else(|| {
        language.pick("the item is impeded", "el ítem está impedido", "o item está impedido").to_owned()
    });
    if let Some(shown) = missing_base(item, &reason, language) {
        return shown;
    }
    let connect = reason
        .strip_prefix(NEEDS_CONNECTION)
        .and_then(|wanted| offering(known.descriptors, wanted.trim()))
        .map(mark)
        .or_else(|| reconnect(known, &reason))
        .unwrap_or(Value::Null);
    let said = said(&reason, &connect);
    json!({ "reason": headline(&reason, &connect, language), "detail": reason, "connect": connect, "said": said, "base": null })
}

fn key(key: &str, vars: Value) -> Value {
    json!({ "key": key, "vars": vars })
}

pub fn said(reason: &str, connect: &Value) -> Value {
    let named = connect["name"].as_str().map(|name| json!({ "name": name }));
    if reason.starts_with(NEEDS_RECONNECT) {
        return named.map_or_else(|| key("reconnect_account", json!({})), |vars| key("reconnect_named", vars));
    }
    if reason.starts_with(NEEDS_CONNECTION) {
        return named.map_or_else(|| key("connect_account", json!({})), |vars| key("connect_named", vars));
    }
    if orchestrator::is_workflow_changed(reason) {
        return key("workflow_changed", json!({}));
    }
    if let Some((_, base)) = orchestrator::missing_base(reason) {
        return key("base_missing", json!({ "base": base }));
    }
    if let Some(reached) = orchestrator::ceiling::budget_reached(reason) {
        return crate::ceiling_view::said(&reached);
    }
    Value::Null
}

const MOST_HEADLINE: usize = 48;

pub fn headline(reason: &str, connect: &Value, language: Language) -> String {
    if let Some(reached) = orchestrator::ceiling::budget_reached(reason) {
        return crate::ceiling_view::headline(&reached, language);
    }
    let name = connect["name"].as_str();
    if reason.starts_with(NEEDS_RECONNECT) {
        return name.map_or_else(
            || language.pick("Reconnect the account", "Reconecta la cuenta", "Reconecte a conta").to_owned(),
            |name| {
                language.pick(format!("Reconnect {name}"), format!("Reconecta {name}"), format!("Reconecte o {name}"))
            },
        );
    }
    if reason.starts_with(NEEDS_CONNECTION) {
        return name.map_or_else(
            || language.pick("Connect an account", "Conecta una cuenta", "Conecte uma conta").to_owned(),
            |name| language.pick(format!("Connect {name}"), format!("Conecta {name}"), format!("Conecte o {name}")),
        );
    }
    if orchestrator::is_workflow_changed(reason) {
        return language.pick("The workflow changed", "El workflow cambió", "O workflow mudou").to_owned();
    }
    if let Some((_, base)) = orchestrator::missing_base(reason) {
        return language.pick(
            format!("Base branch {base} missing"),
            format!("Falta la rama base {base}"),
            format!("Falta a branch base {base}"),
        );
    }
    let head = reason.split_once(": ").map_or(reason, |(head, _)| head).trim();
    let mut short: String = head.chars().take(MOST_HEADLINE).collect();
    if head.chars().count() > MOST_HEADLINE {
        short.push('…');
    }
    let mut letters = short.chars();
    letters.next().map(|first| first.to_uppercase().chain(letters).collect()).unwrap_or_default()
}

fn short_reason(known: &Known<'_>, reason: &str, language: Language) -> (String, Value) {
    let connect = reconnect(known, reason).unwrap_or(Value::Null);
    (headline(reason, &connect, language), said(reason, &connect))
}

fn impeded(language: Language) -> String {
    language.pick("Impeded", "Impedido", "Impedido").to_owned()
}

fn attention(summary: &ItemSummary, language: Language) -> Value {
    let waiting = summary.steps.iter().find(|step| step.state == StepState::WaitingHuman).map(|step| step.id.clone());
    let at = summary.current.clone().or_else(|| waiting.clone());
    if summary.asks > 0 && summary.plans == summary.asks {
        let action = at.map(|step| json!({ "label": language.pick("Review plan", "Revisar plan", "Revisar plano"), "icon": "sPlan", "step": step, "said": key("review_plan", json!({})) }));
        let badge = json!({ "text": language.pick("Plan ready", "Plan listo", "Plano pronto"), "tone": "info", "icon": "sPlan", "said": key("plan_ready", json!({})) });
        return json!({ "badge": badge, "diff": null, "action": action });
    }
    if summary.asks > 0 {
        let action = at.map(|step| json!({ "label": language.pick("Answer", "Responder", "Responder"), "icon": "chat", "step": step, "said": key("answer", json!({})) }));
        let badge = json!({ "text": language.pick("Agent question", "Pregunta del agente", "Pergunta do agente"), "tone": "warning", "icon": "help", "said": key("agent_question", json!({})) });
        return json!({ "badge": badge, "diff": null, "action": action });
    }
    if let Some(step) = waiting {
        let badge = json!({ "text": language.pick("Waiting for you", "Esperándote", "Esperando você"), "tone": "info", "said": key("waiting_you", json!({})) });
        let action =
            json!({ "label": language.pick("Open", "Abrir", "Abrir"), "step": step, "said": key("open", json!({})) });
        return json!({ "badge": badge, "diff": null, "action": action });
    }
    if let Some(found) = &summary.impediment {
        let (text, said) = found.reason.as_deref().map_or_else(
            || (impeded(language), key("impeded", json!({}))),
            |reason| (headline(reason, &Value::Null, language), said(reason, &Value::Null)),
        );
        return json!({ "badge": { "text": text, "tone": "danger", "said": said }, "diff": null, "action": null });
    }
    Value::Null
}

fn activity(known: &Known<'_>, summary: &ItemSummary, language: Language) -> Value {
    if let Some(found) = &summary.impediment {
        let since = iso(summary.updated_at);
        let (text, said) = found.reason.as_deref().map_or_else(
            || (impeded(language), key("impeded", json!({}))),
            |reason| short_reason(known, reason, language),
        );
        return json!({ "text": text, "since": since, "tone": "danger", "said": said });
    }
    let Some(current) = summary.steps.iter().find(|step| Some(&step.id) == summary.current.as_ref()) else {
        return Value::Null;
    };
    let meta = meta(known, current);
    let text = meta["doing"].as_str().or(meta["label"].as_str()).unwrap_or(&current.id);
    json!({ "text": text, "since": iso(current.since.unwrap_or(summary.updated_at)) })
}

pub(crate) fn title(summary: &ItemSummary, language: Language) -> String {
    let chosen = summary.title.trim();
    if !chosen.is_empty() {
        return chosen.to_owned();
    }
    summary
        .issue
        .clone()
        .unwrap_or_else(|| language.pick("Untitled item", "Ítem sin título", "Item sem título").to_owned())
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

fn application(item: &WorkItem) -> Value {
    let chosen = &item.input["application"];
    match chosen["id"].as_str().filter(|id| !id.is_empty()) {
        Some(id) => json!({ "id": id, "name": chosen["name"].as_str().unwrap_or(id) }),
        None => Value::Null,
    }
}

pub fn item(known: &Known<'_>, summary: &ItemSummary, item: Option<&WorkItem>) -> Value {
    let summary = &shown_steps(summary);
    let language = language_of(known, item);
    json!({
        "id": summary.id.as_str(),
        "title": title(summary, language),
        "untitled": summary.title.trim().is_empty() && summary.issue.is_none(),
        "workflow": summary.workflow,
        "group": group(summary.group),
        "status": summary.status,
        "issue": issue(known, summary, item),
        "change": crate::change_view::change(known, summary, item),
        "delivered": crate::delivered_view::delivered(known, summary, item),
        "sources": sources(known, item),
        "git": git(item),
        "steps": summary.steps.iter().map(|step| step_json(known, step)).collect::<Vec<_>>(),
        "current": summary.current,
        "live": summary.live,
        "activity": activity(known, summary, language),
        "impediment": impediment(known, summary, item, language),
        "attention": attention(summary, language),
        "recap": item.and_then(|item| item.recap.as_ref()).map(|recap| recap.text.clone()),
        "createdAt": iso(summary.created_at),
        "updatedAt": iso(summary.updated_at),
        "doneAt": summary.ended_at.map(iso),
        "links": item.map(|item| item.links.iter().map(ItemId::as_str).collect::<Vec<_>>()).unwrap_or_default(),
        "trail": item.map(trail).unwrap_or(Value::Null),
        "accepting": item.is_some_and(|item| item.accepting && !item.is_ended()),
        "application": item.map(application).unwrap_or(Value::Null),
        "joinable": item.is_some_and(joinable),
        "running": item.map(WorkItem::running_agents).unwrap_or_default(),
        "sensitivity": crate::sensitivity_view::sensitivity(item),
    })
}

fn joinable(item: &WorkItem) -> bool {
    !item.is_ended()
        && item.steps.iter().any(|run| {
            run.state == StepState::WaitingHuman
                && run.view.as_array().is_some_and(|blocks| {
                    blocks.iter().any(|block| block["type"] == "members" && block["removable"] == true)
                })
        })
}

fn trail(item: &WorkItem) -> Value {
    let found = item.steps.iter().rev().find_map(|run| {
        run.view.as_array()?.iter().find(|block| block["type"] == "stages").map(|block| block["stages"].clone())
    });
    found.unwrap_or(Value::Null)
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
        let known = Known {
            connections: &connections,
            stale: &stale,
            descriptors: &[],
            described: &Value::Null,
            output: None,
            language: Language::default(),
        };
        let reason = format!("{NEEDS_RECONNECT}refresh_token is invalid");
        assert_eq!(reconnect(&known, &reason).map(|found| found["provider"].clone()), Some(json!("jira")));
        assert_eq!(reconnect(&known, "outra falha"), None);
    }

    #[test]
    fn an_impediment_reads_as_a_few_words_and_keeps_the_detail_for_the_step() {
        let jira = json!({ "provider": "jira", "name": "Jira" });
        let expired = format!("{NEEDS_RECONNECT}connector: Auth: the access could not be renewed");
        assert_eq!(headline(&expired, &jira, Language::Portuguese), "Reconecte o Jira");
        assert_eq!(headline(&expired, &jira, Language::English), "Reconnect Jira");
        assert_eq!(headline(&expired, &Value::Null, Language::Spanish), "Reconecta la cuenta");
        assert_eq!(
            headline(
                "Develop falhou: a pasta de trabalho não é um repositório git",
                &Value::Null,
                Language::Portuguese
            ),
            "Develop falhou"
        );
        assert_eq!(
            headline("o código do workflow mudou e não reproduz o histórico", &Value::Null, Language::Portuguese),
            "O workflow mudou"
        );
        assert!(headline(&"x".repeat(200), &Value::Null, Language::Portuguese).chars().count() <= MOST_HEADLINE + 1);
        assert_eq!(said(&expired, &jira), json!({ "key": "reconnect_named", "vars": { "name": "Jira" } }));
        assert_eq!(said(&expired, &Value::Null), json!({ "key": "reconnect_account", "vars": {} }));
        assert_eq!(said("Develop falhou: x", &Value::Null), Value::Null);
    }

    #[test]
    fn a_missing_base_branch_names_the_repository_and_offers_to_fix_it() {
        let item = WorkItem {
            input: json!({ "application": { "id": "a", "repositories": [
                { "id": "r1", "bindings": { "code": { "value": "acme/web" } } }
            ] } }),
            ..WorkItem::default()
        };
        let reason = orchestrator::blocked_base("r1", "develop");
        let shown = missing_base(Some(&item), &reason, Language::Portuguese).expect("shown");
        assert_eq!(shown["reason"], "A branch base develop não existe em acme/web");
        assert_eq!(shown["base"], json!({ "repository": "r1", "name": "acme/web", "missing": "develop" }));
        assert_eq!(
            shown["said"],
            json!({ "key": "base_missing_in", "vars": { "base": "develop", "name": "acme/web" } })
        );
        assert_eq!(said(&reason, &Value::Null)["key"], "base_missing");
        assert!(missing_base(Some(&item), "something else", Language::English).is_none());
    }
}
