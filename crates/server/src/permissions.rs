use std::sync::Arc;

use axum::Json;
use axum::extract::State;
use axum::response::Response;
use domain::permission::{Allowed, Grant, allowed};
use serde_json::{Value, json};
use store::Store;

use crate::error::{ApiError, ApiResult, json as reply, said};
use crate::state::Ctx;

const KEY: &str = "permissions.allowed";
const SEEDED: &str = "permissions.defaults";

pub fn kept(store: &Store) -> Allowed {
    let saved = store.preference(KEY).ok().flatten().unwrap_or(Value::Null);
    serde_json::from_value::<Vec<Grant>>(saved).map(Allowed::of).unwrap_or_default()
}

fn seeded(store: &Store) -> Allowed {
    let mut allowed = kept(store);
    if store.preference(SEEDED).ok().flatten().is_some() {
        return allowed;
    }
    for grant in allowed::defaults() {
        allowed.add(grant);
    }
    let saved = store.set_preference(KEY, &json!(allowed)).and_then(|()| store.set_preference(SEEDED, &json!(true)));
    if let Err(error) = saved {
        tracing::warn!(%error, "the default allowed list could not be saved");
    }
    allowed
}

pub fn install(store: Arc<Store>) {
    let initial = seeded(&store);
    allowed::install(
        initial,
        Arc::new(move |changed: &Allowed| {
            if let Err(error) = store.set_preference(KEY, &json!(changed)) {
                tracing::warn!(%error, "the allowed list could not be saved");
            }
        }),
    );
}

fn listed(allowed: &Allowed) -> Value {
    json!({ "grants": allowed })
}

fn current(store: &Store) -> Allowed {
    allowed::global().unwrap_or_else(|| kept(store))
}

pub fn replaced(store: &Store, body: &Value) -> ApiResult<Allowed> {
    let grants = serde_json::from_value::<Vec<Grant>>(body["grants"].clone()).map_err(|_| {
        ApiError::bad(said(
            "Send grants as a list of { kind: command | site | tool, value }.",
            "Envía grants como una lista de { kind: command | site | tool, value }.",
            "Envie grants como uma lista de { kind: command | site | tool, value }.",
        ))
    })?;
    let wanted = Allowed::of(grants);
    let changed = allowed::change(|allowed| *allowed = wanted.clone());
    match changed {
        Some(changed) => Ok(changed),
        None => {
            store.set_preference(KEY, &json!(wanted))?;
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
mod tests {
    use super::*;

    #[test]
    fn the_list_is_tidied_and_kept_in_the_store() {
        let store = Store::in_memory(0).expect("store");
        let body = json!({ "grants": [
            { "kind": "command", "value": "  cargo   test " },
            { "kind": "site", "value": "https://www.gov.br/pt-br" },
            { "kind": "site", "value": "www.gov.br" },
            { "kind": "tool", "value": "" },
        ] });
        let saved = replaced(&store, &body).expect("saved");
        assert_eq!(saved.grants(), [Grant::Command("cargo test".into()), Grant::Site("www.gov.br".into())]);
        assert_eq!(kept(&store), saved);
        assert!(replaced(&store, &json!({ "grants": [{ "kind": "nope", "value": "x" }] })).is_err());
    }

    #[test]
    fn the_defaults_come_once_and_keep_what_was_saved() {
        let store = Store::in_memory(0).expect("store");
        replaced(&store, &json!({ "grants": [{ "kind": "site", "value": "gov.br" }] })).expect("saved");
        let first = seeded(&store);
        assert!(first.grants().contains(&Grant::Site("gov.br".into())));
        assert!(first.grants().contains(&Grant::Command("git status".into())));
        replaced(&store, &json!({ "grants": [] })).expect("emptied");
        assert!(seeded(&store).grants().is_empty());
    }
}
