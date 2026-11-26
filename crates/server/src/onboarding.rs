use axum::Json;
use axum::extract::State;
use axum::response::Response;
use serde_json::{Value, json};

use crate::error::{ApiError, ApiResult, json as reply, said};
use crate::state::Ctx;

const SKIPPED: &str = "onboarding.skipped";

fn status_of(ctx: &Ctx) -> ApiResult<Value> {
    let skipped = ctx.backend.store.preference(SKIPPED)?.and_then(|value| value.as_bool()).unwrap_or(false);
    Ok(json!({ "skipped": skipped }))
}

pub async fn get(State(ctx): State<Ctx>) -> ApiResult<Response> {
    Ok(reply(status_of(&ctx)?))
}

pub async fn put(State(ctx): State<Ctx>, Json(patch): Json<Value>) -> ApiResult<Response> {
    let skipped = patch.get("skipped").and_then(Value::as_bool).ok_or_else(|| {
        ApiError::bad(said(
            "Send skipped as true or false.",
            "Envía skipped como true o false.",
            "Informe skipped como true ou false.",
        ))
    })?;
    ctx.backend.store.set_preference(SKIPPED, &json!(skipped))?;
    Ok(reply(status_of(&ctx)?))
}
