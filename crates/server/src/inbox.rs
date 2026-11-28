use std::collections::HashMap;
use std::sync::Arc;

use axum::extract::State;
use axum::response::Response;
use axum::{Extension, Json};
use domain::principal::Principal;
use domain::workflow::{ItemId, StepState, WorkItem};
use domain::{AskKind, Language};
use orchestrator::{ItemSummary, PendingAsk};
use serde_json::{Value, json};
use store::ConnectionRecord;

use crate::blocks::{self, Place};
use crate::error::{ApiError, ApiResult, done, json as reply, said};
use crate::item_view::{self, Known};
use crate::marks::iso;
use crate::session_view;
use crate::state::{Ctx, Shared};

const DEEPEST: usize = 16;

struct Inbox<'a> {
    shared: &'a Shared,
    owner: Option<&'a str>,
    connections: Vec<ConnectionRecord>,
    items: HashMap<ItemId, Option<(WorkItem, ItemSummary)>>,
    described: HashMap<String, Value>,
}

impl<'a> Inbox<'a> {
    fn new(shared: &'a Shared, owner: Option<&'a str>) -> ApiResult<Inbox<'a>> {
        let connections = shared.backend.store.connections()?;
        Ok(Inbox { shared, owner, connections, items: HashMap::new(), described: HashMap::new() })
    }

    fn known(&mut self, id: &ItemId) -> ApiResult<Option<(WorkItem, ItemSummary)>> {
        if let Some(found) = self.items.get(id) {
            return Ok(found.clone());
        }
        let orchestrator = &self.shared.backend.orchestrator;
        let found = match (orchestrator.item(id)?, orchestrator.summary(id)?) {
            (Some(item), Some(summary)) => Some((item, summary)),
            _ => None,
        };
        self.items.insert(id.clone(), found.clone());
        Ok(found)
    }

    fn mine(&mut self, id: &ItemId) -> ApiResult<Option<(WorkItem, ItemSummary)>> {
        let Some((item, summary)) = self.known(id)? else { return Ok(None) };
        let owned = self.owner.is_none_or(|owner| orchestrator::owners::of(&self.shared.backend.store, &item) == owner);
        Ok((owned && !item.is_ended()).then_some((item, summary)))
    }

    fn root(&mut self, summary: &ItemSummary) -> ApiResult<ItemId> {
        let mut at = summary.id.clone();
        let mut parent = summary.parent.clone();
        for _ in 0..DEEPEST {
            let Some(up) = parent else { break };
            parent = self.known(&up)?.and_then(|(_, found)| found.parent);
            at = up;
        }
        Ok(at)
    }

    fn label(&mut self, summary: &ItemSummary, step: &str) -> Value {
        let workflow = summary.workflow.clone();
        let orchestrator = &self.shared.backend.orchestrator;
        let described = self.described.entry(workflow.clone()).or_insert_with(|| orchestrator.describe(&workflow));
        let Some(found) = summary.steps.iter().find(|known| known.id == step) else { return Value::Null };
        let known = Known {
            connections: &[],
            stale: &[],
            descriptors: &[],
            described,
            output: None,
            language: Language::default(),
        };
        let meta = item_view::meta(&known, found);
        json!(meta["label"].as_str().unwrap_or(step))
    }

    fn place(&mut self, item: &WorkItem, summary: &ItemSummary, step: &str) -> ApiResult<Value> {
        let language = language_of(self.shared, item);
        Ok(json!({
            "id": summary.id.as_str(),
            "title": item_view::title(summary, language),
            "workflow": summary.workflow,
            "open": self.root(summary)?.as_str(),
            "step": step,
            "stepLabel": self.label(summary, step),
        }))
    }

    fn ask(&mut self, pending: &PendingAsk) -> ApiResult<Option<Value>> {
        let Some((item, summary)) = self.mine(&pending.item)? else { return Ok(None) };
        let shown = self.place(&item, &summary, &pending.step)?;
        Ok(Some(json!({
            "id": pending.ask.id.0,
            "kind": ask_kind(&pending.ask.kind),
            "item": shown,
            "session": pending.session,
            "since": iso(pending.since),
            "event": session_view::ask_event(&pending.ask, pending.since),
        })))
    }

    fn waits(&mut self, item: &WorkItem) -> ApiResult<Vec<Value>> {
        let Some((item, summary)) = self.mine(&item.id)? else { return Ok(Vec::new()) };
        let language = language_of(self.shared, &item);
        let mut seen: Vec<&str> = Vec::new();
        let mut found = Vec::new();
        for run in item.steps.iter().filter(|run| run.state == StepState::WaitingHuman) {
            if seen.contains(&run.id.as_str()) {
                continue;
            }
            seen.push(&run.id);
            let place = Place { connections: &self.connections, waiting: true, language };
            let shown = blocks::normalized(&place, &run.view);
            let since = summary.steps.iter().find(|step| step.id == run.id).and_then(|step| step.since);
            found.push(json!({
                "id": format!("{}:{}", item.id.as_str(), run.id),
                "kind": wait_kind(&shown),
                "item": self.place(&item, &summary, &run.id)?,
                "session": null,
                "since": iso(since.unwrap_or(summary.updated_at)),
                "blocks": shown,
            }));
        }
        Ok(found)
    }
}

fn language_of(shared: &Shared, item: &WorkItem) -> Language {
    item.input["language"].as_str().and_then(Language::named).unwrap_or(shared.language())
}

fn ask_kind(kind: &AskKind) -> &'static str {
    match kind {
        AskKind::Permission { .. } => "permission",
        AskKind::Questions(_) => "question",
        AskKind::Plan { .. } => "plan",
    }
}

fn wait_kind(shown: &[Value]) -> &'static str {
    let buttons = |block: &Value| block["buttons"].as_array().map_or(0, Vec::len);
    let plain = shown.iter().all(|block| match block["type"].as_str() {
        Some("markdown") => true,
        Some("actions") => buttons(block) <= 1,
        _ => false,
    });
    if plain { "notice" } else { "review" }
}

pub fn pending(shared: &Shared, owner: Option<&str>) -> ApiResult<Vec<Value>> {
    let mut inbox = Inbox::new(shared, owner)?;
    let mut found = Vec::new();
    for pending in shared.backend.orchestrator.asks() {
        found.extend(inbox.ask(&pending)?);
    }
    let waiting = |item: &WorkItem| item.steps.iter().any(|run| run.state == StepState::WaitingHuman);
    for item in shared.backend.store.active_items()?.iter().filter(|item| waiting(item)) {
        found.extend(inbox.waits(item)?);
    }
    found.sort_by(|left, right| right["since"].as_str().cmp(&left["since"].as_str()));
    Ok(found)
}

pub async fn list(State(ctx): State<Ctx>, Extension(person): Extension<Principal>) -> ApiResult<Response> {
    let shared = Arc::clone(&ctx);
    let owner = crate::scope::shared_mode().then(|| person.owner().to_owned());
    let found = crate::sessions::blocking(move || pending(&shared, owner.as_deref())).await?;
    Ok(reply(found))
}

fn gone() -> ApiError {
    ApiError::missing(said(
        "That request was already answered or does not exist.",
        "Esa solicitud ya fue respondida o no existe.",
        "Esse pedido já foi respondido ou não existe.",
    ))
}

fn answer_here(shared: &Shared, pending: &PendingAsk, response: &Value) -> ApiResult<()> {
    let decision = session_view::decision(&pending.ask.kind, response).ok_or_else(|| {
        ApiError::bad(said(
            "That answer does not fit that request.",
            "Esa respuesta no sirve para esa solicitud.",
            "Essa resposta não serve a esse pedido.",
        ))
    })?;
    let resolved = session_view::resolved(session_view::ask_event(&pending.ask, pending.since), response);
    shared.backend.orchestrator.answer_ask_with(&pending.ask.id, &decision, resolved)?;
    Ok(())
}

fn open_ask(shared: &Shared, person: &Principal, id: &str) -> ApiResult<PendingAsk> {
    let asks = shared.backend.orchestrator.asks();
    let pending = asks.into_iter().find(|pending| pending.ask.id.0 == id).ok_or_else(gone)?;
    let owner = crate::scope::item_owner(shared, pending.item.as_str())?;
    let theirs = crate::scope::shared_mode() && owner.as_deref() != Some(person.owner());
    if theirs { Err(gone()) } else { Ok(pending) }
}

pub async fn respond(
    State(ctx): State<Ctx>,
    Extension(person): Extension<Principal>,
    Json(body): Json<Value>,
) -> ApiResult<Response> {
    let id = body["ask"].as_str().unwrap_or_default().to_owned();
    let pending = open_ask(&ctx, &person, &id)?;
    let shared = Arc::clone(&ctx);
    let response = body["response"].clone();
    match pending.session.clone() {
        Some(session) => {
            let asked = session.clone();
            crate::sessions::blocking(move || crate::sessions::answer_now(&shared, &asked, &id, &response)).await?;
            ctx.follow(&session);
        }
        None => crate::sessions::blocking(move || answer_here(&shared, &pending, &response)).await?,
    }
    Ok(done())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_wait_with_only_text_and_one_button_is_a_notice() {
        let notice = [json!({ "type": "markdown", "text": "x" }), json!({ "type": "actions", "buttons": [{}] })];
        assert_eq!(wait_kind(&notice), "notice");
        let review = [json!({ "type": "markdown", "text": "x" }), json!({ "type": "actions", "buttons": [{}, {}] })];
        assert_eq!(wait_kind(&review), "review");
        assert_eq!(wait_kind(&[json!({ "type": "review" })]), "review");
    }
}
