use axum::extract::{Path, State};
use axum::response::Response;
use domain::workflow::ItemId;
use serde_json::json;

use crate::error::{ApiError, ApiResult, done, json as reply, said};
use crate::state::Ctx;

pub async fn item(State(ctx): State<Ctx>, Path(id): Path<String>) -> ApiResult<Response> {
    let id = ItemId::from(id.as_str());
    if ctx.backend.orchestrator.item(&id)?.is_none() {
        return Err(ApiError::missing(said("There is no such item.", "No existe ese ítem.", "Não há esse item.")));
    }
    ctx.backend.orchestrator.kill(&id)?;
    Ok(done())
}

pub async fn agent(State(ctx): State<Ctx>, Path(name): Path<String>) -> ApiResult<Response> {
    let killed = ctx.backend.orchestrator.kill_agent(&name)?;
    let stopped: Vec<&str> = killed.stopped.iter().map(ItemId::as_str).collect();
    let failed: Vec<_> =
        killed.failed.iter().map(|(id, error)| json!({ "item": id.as_str(), "error": error })).collect();
    Ok(reply(json!({ "stopped": stopped, "failed": failed })))
}
