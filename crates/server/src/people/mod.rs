mod roles;

pub use roles::{assign, roles};

use std::collections::HashMap;

use axum::extract::{Path, State};
use axum::response::Response;
use domain::principal::UserId;
use serde_json::{Value, json};

use crate::error::{ApiResult, done, json as reply};
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
            let standing = crate::roles::standing_from(crate::roles::roles(), listed, &all);
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

pub async fn sign_out(State(ctx): State<Ctx>, Path(id): Path<String>) -> ApiResult<Response> {
    let closed = ctx.backend.store.close_sessions_of(&UserId(id.clone()))?;
    tracing::info!(target: "audit", user = %id, closed, "auth.signed_out_by_admin");
    Ok(done())
}
