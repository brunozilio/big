use std::collections::HashMap;

use axum::Json;
use axum::extract::{Path, State};
use axum::response::Response;
use domain::principal::UserId;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::error::{ApiError, ApiResult, done, json as reply, said};
use crate::roles::{ASSIGNED, Grant, SSO};
use crate::state::Ctx;

fn held_by(ctx: &Ctx) -> ApiResult<HashMap<String, Vec<(String, String)>>> {
    let mut held: HashMap<String, Vec<(String, String)>> = HashMap::new();
    for role in ctx.backend.store.all_roles()? {
        held.entry(role.user.0).or_default().push((role.source, role.role));
    }
    Ok(held)
}

fn from(held: &[(String, String)], source: &str) -> Vec<String> {
    held.iter().filter(|(kept, _)| kept == source).map(|(_, role)| role.clone()).collect()
}

pub async fn list(State(ctx): State<Ctx>) -> ApiResult<Response> {
    let now = crate::auth::settings::now();
    let admins = crate::scope::admins();
    let held = held_by(&ctx)?;
    let people: Vec<Value> = ctx
        .backend
        .store
        .people(now)?
        .into_iter()
        .map(|person| {
            let kept = held.get(person.principal.user.as_str()).map(Vec::as_slice).unwrap_or_default();
            let listed = admins.contains(&person.principal.email);
            let all: Vec<String> = kept.iter().map(|(_, role)| role.clone()).collect();
            let standing = crate::scope::standing_from(crate::scope::roles(), listed, &all);
            json!({
                "id": person.principal.user,
                "email": person.principal.email,
                "name": person.principal.name,
                "created": person.created,
                "seen": person.seen,
                "sessions": person.sessions,
                "admin": standing.grants.has(Grant::Admin),
                "listedAdmin": listed,
                "roles": standing.roles,
                "assigned": from(kept, ASSIGNED),
                "sso": from(kept, SSO),
            })
        })
        .collect();
    Ok(reply(people))
}

pub async fn roles() -> ApiResult<Response> {
    let roles = crate::scope::roles();
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
    let roles = crate::scope::roles();
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

pub async fn sign_out(State(ctx): State<Ctx>, Path(id): Path<String>) -> ApiResult<Response> {
    let closed = ctx.backend.store.close_sessions_of(&UserId(id.clone()))?;
    tracing::info!(target: "audit", user = %id, closed, "auth.signed_out_by_admin");
    Ok(done())
}
