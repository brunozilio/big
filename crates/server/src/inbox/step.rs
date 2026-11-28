use axum::extract::{Path, State};
use axum::response::Response;
use axum::{Extension, Json};
use domain::principal::Principal;
use domain::workflow::ItemId;
use serde_json::Value;

use super::answer::guard_wait;
use crate::blocks;
use crate::error::{ApiError, ApiResult, done, said};
use crate::state::Ctx;

pub async fn human(
    State(ctx): State<Ctx>,
    Extension(person): Extension<Principal>,
    Path((id, step)): Path<(String, String)>,
    Json(body): Json<Value>,
) -> ApiResult<Response> {
    let id = ItemId::from(id);
    let action = body["action"].as_str().filter(|action| !action.is_empty());
    let action = action.ok_or_else(|| ApiError::bad(said("Say the action.", "Indica la acción.", "Diga a ação.")))?;
    let (action, payload) = blocks::input(action, body["payload"].clone());
    guard_wait(&ctx, &person, &id, &step)?;
    let by = [person.name.trim(), person.email.trim()].into_iter().find(|name| !name.is_empty()).unwrap_or("human");
    ctx.backend.orchestrator.human_input_by(&id, &step, &action, payload, by)?;
    if let Err(error) = ctx.backend.store.release_inbox(&super::wait_id(&id, &step), None) {
        tracing::warn!(%error, "the claim of an answered step could not be dropped");
    }
    Ok(done())
}
