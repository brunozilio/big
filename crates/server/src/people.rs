use axum::extract::{Path, State};
use axum::response::Response;
use domain::principal::UserId;
use serde_json::{Value, json};

use crate::error::{ApiResult, done, json as reply};
use crate::state::Ctx;

pub async fn list(State(ctx): State<Ctx>) -> ApiResult<Response> {
    let now = crate::auth::settings::now();
    let admins = crate::scope::admins();
    let people: Vec<Value> = ctx
        .backend
        .store
        .people(now)?
        .into_iter()
        .map(|person| {
            json!({
                "id": person.principal.user,
                "email": person.principal.email,
                "name": person.principal.name,
                "created": person.created,
                "seen": person.seen,
                "sessions": person.sessions,
                "admin": admins.contains(&person.principal.email),
            })
        })
        .collect();
    Ok(reply(people))
}

pub async fn sign_out(State(ctx): State<Ctx>, Path(id): Path<String>) -> ApiResult<Response> {
    let closed = ctx.backend.store.close_sessions_of(&UserId(id.clone()))?;
    tracing::info!(target: "audit", user = %id, closed, "auth.signed_out_by_admin");
    Ok(done())
}
