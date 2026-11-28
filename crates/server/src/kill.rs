use axum::extract::{Path, State};
use axum::response::Response;
use domain::effect::Effect;
use domain::workflow::{ItemId, WorkItem};
use serde_json::json;

use crate::error::{ApiError, ApiResult, done, json as reply, said};
use crate::state::Ctx;

pub fn running(item: &WorkItem) -> Vec<String> {
    let mut agents: Vec<String> = item
        .effects
        .values()
        .filter(|entry| entry.outcome.is_none() && !item.is_ended())
        .filter_map(|entry| match &entry.request.effect {
            Effect::AgentRun { agent, .. } => Some(agent.clone()),
            _ => None,
        })
        .collect();
    agents.sort();
    agents.dedup();
    agents
}

pub async fn item(State(ctx): State<Ctx>, Path(id): Path<String>) -> ApiResult<Response> {
    let id = ItemId::from(id.as_str());
    if ctx.backend.orchestrator.item(&id)?.is_none() {
        return Err(ApiError::missing(said("There is no such item.", "No existe ese ítem.", "Não há esse item.")));
    }
    ctx.backend.orchestrator.kill(&id)?;
    Ok(done())
}

pub async fn agent(State(ctx): State<Ctx>, Path(name): Path<String>) -> ApiResult<Response> {
    let stopped = ctx.backend.orchestrator.kill_agent(&name)?;
    let items: Vec<&str> = stopped.iter().map(ItemId::as_str).collect();
    Ok(reply(json!({ "stopped": items })))
}
