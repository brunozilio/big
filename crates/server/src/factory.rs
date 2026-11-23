use axum::extract::State;
use axum::response::Response;

use crate::error::{ApiResult, json as reply};
use crate::state::Ctx;

pub async fn report(State(ctx): State<Ctx>) -> ApiResult<Response> {
    let orchestrator = ctx.backend.orchestrator.clone();
    let report = tokio::task::spawn_blocking(move || orchestrator.factory())
        .await
        .map_err(crate::error::ApiError::failed)??;
    Ok(reply(report))
}
