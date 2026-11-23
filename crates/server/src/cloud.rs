use axum::extract::{Path, State};
use axum::response::Response;

use crate::error::{ApiResult, json};
use crate::state::Ctx;

pub fn route() -> String {
    format!("{}{{token}}", orchestrator::CLOUD_CREDENTIALS_PATH)
}

pub async fn credentials(State(ctx): State<Ctx>, Path(token): Path<String>) -> ApiResult<Response> {
    Ok(json(ctx.backend.orchestrator.cloud_credentials(&token).await?))
}
