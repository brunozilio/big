mod answer;
mod claims;
mod step;

use std::collections::HashMap;
use std::sync::Arc;

use axum::extract::State;
use axum::response::Response;
use axum::Extension;
use domain::principal::Principal;
use domain::workflow::{Assignee, ItemId, StepState, WorkItem};
use domain::{AskKind, Language};
use orchestrator::{ItemSummary, PendingAsk};
use serde_json::{Value, json};
use store::{Claim, ConnectionRecord};

pub use answer::{batch, respond};
pub use step::human;
pub use claims::{claim, release};

use crate::assignees::Reader;
use crate::blocks::{self, Place};
use crate::error::{ApiResult, json as reply};
use crate::item_view::{self, Known};
use crate::marks::iso;
use crate::session_view;
use crate::state::{Ctx, Shared};

const DEEPEST: usize = 16;

struct Inbox<'a> {
    shared: &'a Shared,
    reader: &'a Reader<'a>,
    connections: Vec<ConnectionRecord>,
    claims: HashMap<String, Claim>,
    items: HashMap<ItemId, Option<(WorkItem, ItemSummary)>>,
    described: HashMap<String, Value>,
}

impl<'a> Inbox<'a> {
    fn new(shared: &'a Shared, reader: &'a Reader<'a>) -> ApiResult<Inbox<'a>> {
        let connections = shared.backend.store.connections()?;
        let claims = shared.backend.store.inbox_claims()?.into_iter().map(|held| (held.target.clone(), held)).collect();
        Ok(Inbox { shared, reader, connections, claims, items: HashMap::new(), described: HashMap::new() })
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

    fn seen(&mut self, id: &ItemId, assign: Option<&Assignee>) -> ApiResult<Option<(WorkItem, ItemSummary)>> {
        let Some((item, summary)) = self.known(id)? else { return Ok(None) };
        let owner = orchestrator::owners::of(&self.shared.backend.store, &item);
        Ok((self.reader.sees(&owner, assign) && !item.is_ended()).then_some((item, summary)))
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

    fn routed(&self, entry: &mut Value, id: &str, assign: Option<&Assignee>) {
        entry["assignedTo"] = assign.map_or(Value::Null, Assignee::shown);
        let held = self.claims.get(id).filter(|held| answer::still_held(self.shared, held, assign));
        entry["claim"] = held.map_or(Value::Null, |held| {
            json!({ "by": held.name, "at": iso(held.at), "mine": held.person == self.reader.person.owner() })
        });
    }

    fn ask(&mut self, pending: &PendingAsk) -> ApiResult<Option<Value>> {
        let Some((item, summary)) = self.seen(&pending.item, pending.assign.as_ref())? else { return Ok(None) };
        let shown = self.place(&item, &summary, &pending.step)?;
        let mut entry = json!({
            "id": pending.ask.id.0,
            "kind": ask_kind(&pending.ask.kind),
            "item": shown,
            "session": pending.session,
            "since": iso(pending.since),
            "event": session_view::ask_event(&pending.ask, pending.since),
        });
        self.routed(&mut entry, &pending.ask.id.0, pending.assign.as_ref());
        Ok(Some(entry))
    }

    fn waits(&mut self, item: &WorkItem) -> ApiResult<Vec<Value>> {
        let mut seen: Vec<&str> = Vec::new();
        let mut found = Vec::new();
        for run in item.steps.iter().filter(|run| run.state == StepState::WaitingHuman) {
            if seen.contains(&run.id.as_str()) {
                continue;
            }
            seen.push(&run.id);
            let Some((item, summary)) = self.seen(&item.id, run.assign.as_ref())? else { continue };
            let language = language_of(self.shared, &item);
            let place = Place { connections: &self.connections, waiting: true, language };
            let shown = blocks::normalized(&place, &run.view);
            let since = summary.steps.iter().find(|step| step.id == run.id).and_then(|step| step.since);
            let id = wait_id(&item.id, &run.id);
            let mut entry = json!({
                "id": id,
                "kind": wait_kind(&shown),
                "item": self.place(&item, &summary, &run.id)?,
                "session": null,
                "since": iso(since.unwrap_or(summary.updated_at)),
                "blocks": shown,
            });
            self.routed(&mut entry, &id, run.assign.as_ref());
            found.push(entry);
        }
        Ok(found)
    }
}

pub fn forget_claims(shared: &Shared, item: &ItemId) {
    if let Err(error) = shared.backend.store.forget_item_claims(item.as_str()) {
        tracing::warn!(%error, item = %item, "the claims of an ended item could not be dropped");
    }
}

pub fn wait_id(item: &ItemId, step: &str) -> String {
    format!("{}:{step}", item.as_str())
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

fn pending(shared: &Shared, reader: &Reader<'_>) -> ApiResult<Vec<Value>> {
    let mut inbox = Inbox::new(shared, reader)?;
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
    let found = crate::sessions::blocking(move || pending(&shared, &Reader::of(&shared, &person))).await?;
    Ok(reply(found))
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
