use std::sync::Arc;

use axum::extract::State;
use axum::response::Response;
use axum::{Extension, Json};
use domain::principal::Principal;
use serde_json::{Value, json};
use store::Claim;

use super::answer::{taken, visible};
use crate::assignees::Reader;
use crate::error::{ApiResult, done, json as reply};
use crate::marks::{iso, now_ms};
use crate::state::{Ctx, Shared};

fn claim_now(shared: &Shared, person: &Principal, id: &str) -> ApiResult<Value> {
    let reader = Reader::of(shared, person);
    visible(shared, &reader, id)?;
    let wanted = Claim {
        target: id.to_owned(),
        person: person.owner().to_owned(),
        name: reader.display().to_owned(),
        at: now_ms(),
    };
    let held = shared.backend.store.claim_inbox(&wanted)?;
    if held.person != wanted.person {
        return Err(taken(&held.name));
    }
    tracing::info!(target: "audit", user = person.owner(), request = id, outcome = "claimed", "inbox.claimed");
    Ok(json!({ "id": id, "claim": { "by": held.name, "at": iso(held.at), "mine": true } }))
}

fn release_now(shared: &Shared, person: &Principal, id: &str) -> ApiResult<()> {
    let reader = Reader::of(shared, person);
    visible(shared, &reader, id)?;
    let whose = (!reader.admin()).then(|| person.owner());
    if shared.backend.store.release_inbox(id, whose)? {
        tracing::info!(target: "audit", user = person.owner(), request = id, outcome = "released", "inbox.released");
    }
    Ok(())
}

fn id_of(body: &Value) -> String {
    body["id"].as_str().unwrap_or_default().to_owned()
}

pub async fn claim(
    State(ctx): State<Ctx>,
    Extension(person): Extension<Principal>,
    Json(body): Json<Value>,
) -> ApiResult<Response> {
    let shared = Arc::clone(&ctx);
    let held = crate::sessions::blocking(move || claim_now(&shared, &person, &id_of(&body))).await?;
    Ok(reply(held))
}

pub async fn release(
    State(ctx): State<Ctx>,
    Extension(person): Extension<Principal>,
    Json(body): Json<Value>,
) -> ApiResult<Response> {
    let shared = Arc::clone(&ctx);
    crate::sessions::blocking(move || release_now(&shared, &person, &id_of(&body))).await?;
    Ok(done())
}
