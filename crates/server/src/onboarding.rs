use axum::extract::State;
use axum::{Extension, Json};
use domain::principal::Principal;
use axum::response::Response;
use serde_json::{Value, json};

use crate::error::{ApiError, ApiResult, json as reply, said};
use crate::state::Ctx;

const SKIPPED: &str = "onboarding.skipped";

fn key(owner: &str) -> String {
    if owner == domain::principal::LOCAL_USER { SKIPPED.to_owned() } else { format!("user.{owner}.{SKIPPED}") }
}

fn status_of(ctx: &Ctx, owner: &str) -> ApiResult<Value> {
    let skipped = ctx.backend.store.preference(&key(owner))?.and_then(|value| value.as_bool()).unwrap_or(false);
    Ok(json!({ "skipped": skipped }))
}

pub async fn get(State(ctx): State<Ctx>, Extension(person): Extension<Principal>) -> ApiResult<Response> {
    Ok(reply(status_of(&ctx, person.owner())?))
}

pub async fn put(
    State(ctx): State<Ctx>,
    Extension(person): Extension<Principal>,
    Json(patch): Json<Value>,
) -> ApiResult<Response> {
    let skipped = patch.get("skipped").and_then(Value::as_bool).ok_or_else(|| {
        ApiError::bad(said(
            "Send skipped as true or false.",
            "Envía skipped como true o false.",
            "Informe skipped como true ou false.",
        ))
    })?;
    ctx.backend.store.set_preference(&key(person.owner()), &json!(skipped))?;
    Ok(reply(status_of(&ctx, person.owner())?))
}
