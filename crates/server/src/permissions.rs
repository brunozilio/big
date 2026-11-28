mod store;

use std::sync::Arc;

use axum::Json;
use axum::extract::State;
use axum::response::Response;
use domain::permission::{Allowed, Grant, Rule, allowed};
use serde_json::{Value, json};
use ::store::Store;

pub use self::store::kept;
use self::store::{Kept, persisted, problem, saved, seeded, set_problem};
use crate::error::{ApiError, ApiResult, json as reply, said};
use crate::state::Ctx;

pub fn install(store: Arc<Store>) {
    let Kept { allowed: initial, problem: told } = seeded(&store);
    if told.is_none() {
        persisted(&store, None, &initial);
    }
    set_problem(told);
    allowed::install(
        initial,
        Arc::new(move |changed: &Allowed| {
            persisted(&store, problem().as_ref(), changed);
        }),
    );
}

fn listed(allowed: &Allowed) -> Value {
    json!({ "grants": allowed, "problem": problem() })
}

fn current(store: &Store) -> Allowed {
    allowed::global().unwrap_or_else(|| kept(store))
}

fn refused_loose(rules: &[Rule]) -> ApiResult<()> {
    let Some(loose) = rules.iter().find(|rule| rule.loose()) else { return Ok(()) };
    let value = match &loose.grant {
        Grant::Command(value) | Grant::Site(value) | Grant::Tool(value) => value.trim().to_owned(),
    };
    Err(ApiError::bad(said(
        format!("{value} would allow everything; name the commands or sites, or deny it instead."),
        format!("{value} permitiría todo; nombra los comandos o sitios, o deniégalo."),
        format!("{value} liberaria tudo; nomeie os comandos ou sites, ou negue."),
    )))
}

pub fn replaced(store: &Store, body: &Value) -> ApiResult<Allowed> {
    let grants = serde_json::from_value::<Vec<Rule>>(body["grants"].clone()).map_err(|_| {
        ApiError::bad(said(
            "Send grants as a list of { kind: command | site | tool, value, behavior: allow | ask | deny }.",
            "Envía grants como una lista de { kind: command | site | tool, value, behavior: allow | ask | deny }.",
            "Envie grants como uma lista de { kind: command | site | tool, value, behavior: allow | ask | deny }.",
        ))
    })?;
    refused_loose(&grants)?;
    let wanted = Allowed::of(grants);
    set_problem(None);
    let changed = allowed::change(|allowed| *allowed = wanted.clone());
    match changed {
        Some(changed) => Ok(changed),
        None => {
            saved(store, &wanted)?;
            Ok(wanted)
        }
    }
}

pub async fn get(State(ctx): State<Ctx>) -> Json<Value> {
    Json(listed(&current(&ctx.backend.store)))
}

pub async fn put(State(ctx): State<Ctx>, Json(body): Json<Value>) -> ApiResult<Response> {
    let saved = replaced(&ctx.backend.store, &body)?;
    Ok(reply(listed(&saved)))
}

#[cfg(test)]
#[path = "permissions/handler_tests.rs"]
mod tests;
