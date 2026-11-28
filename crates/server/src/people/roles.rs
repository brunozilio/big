use axum::Json;
use axum::extract::{Path, State};
use axum::response::Response;
use domain::principal::UserId;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::error::{ApiError, ApiResult, done, json as reply, said};
use crate::roles::ASSIGNED;
use crate::state::Ctx;

pub async fn roles() -> ApiResult<Response> {
    let roles = crate::roles::roles();
    let declared: Vec<Value> = roles
        .declared
        .iter()
        .map(|role| json!({ "name": role.name, "grants": role.grants.iter().map(|grant| grant.key()).collect::<Vec<_>>() }))
        .collect();
    Ok(reply(json!({ "roles": declared, "defaultRole": roles.default_role, "adminRole": roles.admin_role })))
}

#[derive(Debug, Deserialize)]
pub struct Assigned {
    roles: Vec<String>,
}

pub async fn assign(
    State(ctx): State<Ctx>,
    Path(id): Path<String>,
    Json(assigned): Json<Assigned>,
) -> ApiResult<Response> {
    let roles = crate::roles::roles();
    if let Some(unknown) = assigned.roles.iter().find(|role| !roles.knows(role)) {
        return Err(ApiError::bad(said(
            format!("There is no role named {unknown}."),
            format!("No hay un rol llamado {unknown}."),
            format!("Não há papel chamado {unknown}."),
        )));
    }
    let user = UserId(id);
    if ctx.backend.store.user(&user)?.is_none() {
        return Err(ApiError::missing(said(
            "There is no such person.",
            "No existe esa persona.",
            "Essa pessoa não existe.",
        )));
    }
    ctx.backend.store.set_roles(&user, ASSIGNED, &assigned.roles)?;
    tracing::info!(target: "audit", user = %user, roles = %assigned.roles.join(","), "auth.roles_assigned");
    Ok(done())
}

