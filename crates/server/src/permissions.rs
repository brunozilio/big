use std::sync::Arc;

use axum::Json;
use axum::extract::State;
use axum::response::Response;
use domain::permission::{Allowed, Grant, allowed};
use serde_json::{Value, json};
use store::Store;

use crate::error::{ApiError, ApiResult, json as reply};
use crate::state::Ctx;

const KEY: &str = "permissions.allowed";

pub fn kept(store: &Store) -> Allowed {
    let saved = store.preference(KEY).ok().flatten().unwrap_or(Value::Null);
    serde_json::from_value::<Vec<Grant>>(saved).map(Allowed::of).unwrap_or_default()
}

pub fn install(store: Arc<Store>) {
    let initial = kept(&store);
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
    let grants = serde_json::from_value::<Vec<Grant>>(body["grants"].clone())
        .map_err(|_| ApiError::bad("Envie grants como uma lista de { kind: command | site | tool, value }."))?;
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
}
