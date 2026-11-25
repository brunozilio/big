use std::sync::Arc;

use axum::extract::{Path, State};
use axum::response::Response;
use domain::trace::Record;
use domain::workflow::ItemId;
use domain::{EngineEvent, SessionId};
use serde_json::{Value, json};

use crate::error::{ApiError, ApiResult, json as reply};
use crate::state::{Ctx, Shared};

#[derive(Debug, Default, Clone, PartialEq)]
pub struct Spent {
    pub cost_usd: f64,
    pub priced: bool,
    pub input: u64,
    pub output: u64,
    pub cache_read: u64,
    pub cache_write: u64,
    pub calls: u64,
}

#[derive(Debug, Default, Clone, PartialEq)]
pub struct Context {
    pub used: u64,
    pub model: Option<String>,
}

impl Spent {
    fn add(&mut self, event: &EngineEvent) {
        match event {
            EngineEvent::Usage(usage) => {
                if let Some(cost) = usage.estimated_cost_usd {
                    self.cost_usd += cost;
                    self.priced = true;
                }
                self.input += usage.input;
                self.output += usage.output;
                self.cache_read += usage.cache_read;
                self.cache_write += usage.cache_write;
                self.calls += 1;
            }
            EngineEvent::AgentEvent { event, .. } => self.add(event),
            _ => {}
        }
    }

    fn json(&self) -> Value {
        json!({
            "costUsd": self.priced.then_some(self.cost_usd),
            "input": self.input, "output": self.output,
            "cacheRead": self.cache_read, "cacheWrite": self.cache_write,
            "calls": self.calls,
        })
    }
}

impl Context {
    fn see(&mut self, event: &EngineEvent) {
        match event {
            EngineEvent::Usage(usage) => {
                self.used = usage.input + usage.cache_read + usage.cache_write + usage.output;
                self.model = Some(usage.model.clone()).filter(|model| !model.is_empty());
            }
            EngineEvent::Compacted(compaction) => self.used = compaction.after,
            _ => {}
        }
    }
}

pub fn tally(sessions: &[Vec<EngineEvent>]) -> (Spent, Option<Context>) {
    let mut spent = Spent::default();
    let mut context = None;
    for events in sessions {
        let mut seen = Context::default();
        for event in events {
            spent.add(event);
            seen.see(event);
        }
        context = Some(seen);
    }
    (spent, context.filter(|context| context.used > 0))
}

fn events_of(shared: &Shared, session: &str, cwd: &std::path::Path) -> Vec<EngineEvent> {
    let records = match shared.backend.client.history(&SessionId(session.to_owned()), cwd) {
        Ok(records) => records,
        Err(error) => {
            tracing::warn!(%error, session, "the session's history could not be read for the item's usage");
            return Vec::new();
        }
    };
    records
        .into_iter()
        .filter_map(|record| match record {
            Record::Event { mut event, .. } => {
                crate::usage::priced(&mut event);
                Some(event)
            }
            _ => None,
        })
        .collect()
}

fn gather(shared: &Shared, id: &ItemId) -> ApiResult<Value> {
    let orchestrator = &shared.backend.orchestrator;
    if orchestrator.item(id)?.is_none() {
        return Err(orchestrator::Error::UnknownItem(id.to_string()).into());
    }
    let sessions = orchestrator.item_sessions(id)?;
    let histories: Vec<Vec<EngineEvent>> =
        sessions.iter().map(|known| events_of(shared, &known.session, &known.cwd)).collect();
    let (spent, context) = tally(&histories);
    let context = match context {
        Some(context) => {
            let window = orchestrator.context_window(id, context.model.as_deref())?;
            json!({
                "used": context.used,
                "window": window,
                "percent": percent(context.used, window),
            })
        }
        None => Value::Null,
    };
    Ok(json!({ "spent": spent.json(), "context": context }))
}

pub fn percent(used: u64, window: u64) -> u64 {
    used.saturating_mul(100).checked_div(window).map_or(0, |share| share.min(100))
}

pub async fn usage(State(ctx): State<Ctx>, Path(id): Path<String>) -> ApiResult<Response> {
    let shared = Arc::clone(&ctx);
    let id = ItemId::from(id.as_str());
    let found = tokio::task::spawn_blocking(move || gather(&shared, &id)).await.map_err(ApiError::failed)??;
    Ok(reply(found))
}

#[cfg(test)]
#[path = "item_usage_tests.rs"]
mod tests;
