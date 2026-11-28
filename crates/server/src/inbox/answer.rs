use std::sync::Arc;

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::Response;
use axum::{Extension, Json};
use domain::principal::{Principal, UserId};
use domain::workflow::{Assignee, ItemId, StepState};
use orchestrator::PendingAsk;
use serde_json::{Value, json};
use store::Claim;

use crate::assignees::Reader;
use crate::blocks;
use crate::error::{ApiError, ApiResult, done, json as reply, said};
use crate::session_view;
use crate::state::{Ctx, Shared};

pub enum Target {
    Ask(PendingAsk),
    Wait { item: ItemId, step: String, assign: Option<Assignee> },
}

fn gone() -> ApiError {
    ApiError::missing(said(
        "That request was already answered or does not exist.",
        "Esa solicitud ya fue respondida o no existe.",
        "Esse pedido já foi respondido ou não existe.",
    ))
}

pub fn taken(by: &str) -> ApiError {
    ApiError::new(
        StatusCode::CONFLICT,
        "claimed",
        said(
            format!("{by} already took this request."),
            format!("{by} ya tomó esta solicitud."),
            format!("{by} já pegou esse pedido."),
        ),
    )
}

fn waiting(shared: &Shared, id: &str) -> ApiResult<Option<Target>> {
    let Some((item, step)) = id.split_once(':') else { return Ok(None) };
    let item = ItemId::from(item);
    let Some(found) = shared.backend.orchestrator.item(&item)? else { return Ok(None) };
    let run = found.steps.iter().find(|run| run.id == step && run.state == StepState::WaitingHuman);
    Ok(run.map(|run| Target::Wait { item: item.clone(), step: step.to_owned(), assign: run.assign.clone() }))
}

pub fn target(shared: &Shared, id: &str) -> ApiResult<Target> {
    let asks = shared.backend.orchestrator.asks();
    if let Some(pending) = asks.into_iter().find(|pending| pending.ask.id.0 == id) {
        return Ok(Target::Ask(pending));
    }
    waiting(shared, id)?.ok_or_else(gone)
}

impl Target {
    pub fn item(&self) -> &ItemId {
        match self {
            Target::Ask(pending) => &pending.item,
            Target::Wait { item, .. } => item,
        }
    }

    pub fn assign(&self) -> Option<&Assignee> {
        match self {
            Target::Ask(pending) => pending.assign.as_ref(),
            Target::Wait { assign, .. } => assign.as_ref(),
        }
    }
}

pub fn visible(shared: &Shared, reader: &Reader<'_>, id: &str) -> ApiResult<Target> {
    let found = target(shared, id)?;
    let owner = crate::scope::item_owner(shared, found.item().as_str())?.unwrap_or_default();
    if reader.sees(&owner, found.assign()) { Ok(found) } else { Err(gone()) }
}

pub fn still_held(shared: &Shared, held: &Claim, assign: Option<&Assignee>) -> bool {
    let Some(assign) = assign else { return true };
    match shared.backend.store.user(&UserId(held.person.clone())) {
        Ok(Some(claimant)) => Reader::of(shared, &claimant).reaches(assign),
        Ok(None) => false,
        Err(error) => {
            tracing::warn!(%error, "the person holding a claim could not be read");
            true
        }
    }
}

fn free(shared: &Shared, reader: &Reader<'_>, id: &str, assign: Option<&Assignee>) -> ApiResult<()> {
    let claims = shared.backend.store.inbox_claims()?;
    match claims.into_iter().find(|held| held.target == id) {
        Some(held) if held.person != reader.person.owner() && still_held(shared, &held, assign) => {
            Err(taken(&held.name))
        }
        _ => Ok(()),
    }
}

pub fn guard_wait(shared: &Shared, person: &Principal, item: &ItemId, step: &str) -> ApiResult<()> {
    let reader = Reader::of(shared, person);
    let id = super::wait_id(item, step);
    let Some(Target::Wait { assign: Some(assign), .. }) = waiting(shared, &id)? else {
        return free(shared, &reader, &id, None);
    };
    if !reader.reaches(&assign) {
        return Err(crate::scope::not_allowed());
    }
    free(shared, &reader, &id, Some(&assign))
}

fn noted(payload: Value, note: &str) -> Value {
    if note.is_empty() {
        return payload;
    }
    let mut payload = if payload.is_object() { payload } else { json!({}) };
    payload["note"] = json!(note);
    payload
}

fn ask_now(shared: &Shared, pending: &PendingAsk, response: &Value, note: &str) -> ApiResult<()> {
    let decision = session_view::decision(&pending.ask.kind, response).ok_or_else(|| {
        ApiError::bad(said(
            "That answer does not fit that request.",
            "Esa respuesta no sirve para esa solicitud.",
            "Essa resposta não serve a esse pedido.",
        ))
    })?;
    let mut resolved = session_view::resolved(session_view::ask_event(&pending.ask, pending.since), response);
    if !note.is_empty() {
        resolved["note"] = json!(note);
    }
    shared.backend.orchestrator.answer_ask_with(&pending.ask.id, &decision, resolved)?;
    Ok(())
}

fn wait_now(
    shared: &Shared,
    reader: &Reader<'_>,
    (item, step): (&ItemId, &str),
    response: &Value,
    note: &str,
) -> ApiResult<()> {
    let action = response["action"].as_str().filter(|action| !action.is_empty());
    let action = action.ok_or_else(|| ApiError::bad(said("Say the action.", "Indica la acción.", "Diga a ação.")))?;
    let given = if response["payload"].is_null() { response["values"].clone() } else { response["payload"].clone() };
    let (action, payload) = blocks::input(action, given);
    shared.backend.orchestrator.human_input_by(item, step, &action, noted(payload, note), reader.display())?;
    Ok(())
}

pub fn answer(
    shared: &Shared,
    reader: &Reader<'_>,
    id: &str,
    response: &Value,
    note: &str,
) -> ApiResult<Option<String>> {
    let found = visible(shared, reader, id)?;
    free(shared, reader, id, found.assign())?;
    let followed = match &found {
        Target::Ask(pending) => match pending.session.clone() {
            Some(session) => {
                crate::sessions::answer_now(shared, &session, id, response)?;
                Some(session)
            }
            None => {
                ask_now(shared, pending, response, note)?;
                None
            }
        },
        Target::Wait { item, step, .. } => {
            wait_now(shared, reader, (item, step), response, note)?;
            None
        }
    };
    shared.backend.store.release_inbox(id, None)?;
    Ok(followed)
}

fn note_of(body: &Value) -> String {
    body["note"].as_str().unwrap_or_default().trim().to_owned()
}

pub async fn respond(
    State(ctx): State<Ctx>,
    Extension(person): Extension<Principal>,
    Json(body): Json<Value>,
) -> ApiResult<Response> {
    let id = body["ask"].as_str().unwrap_or_default().to_owned();
    let shared = Arc::clone(&ctx);
    let followed = crate::sessions::blocking(move || {
        answer(&shared, &Reader::of(&shared, &person), &id, &body["response"], &note_of(&body))
    })
    .await?;
    if let Some(session) = followed {
        ctx.follow(&session);
    }
    Ok(done())
}

pub async fn batch(
    State(ctx): State<Ctx>,
    Extension(person): Extension<Principal>,
    Json(body): Json<Value>,
) -> ApiResult<Response> {
    let ids: Vec<String> =
        body["ids"].as_array().into_iter().flatten().filter_map(Value::as_str).map(str::to_owned).collect();
    if ids.is_empty() {
        return Err(ApiError::bad(said(
            "Choose at least one request.",
            "Elige al menos una solicitud.",
            "Escolha ao menos um pedido.",
        )));
    }
    let shared = Arc::clone(&ctx);
    let (results, followed) = crate::sessions::blocking(move || {
        let reader = Reader::of(&shared, &person);
        let mut results = Vec::new();
        let mut followed = Vec::new();
        for id in &ids {
            match answer(&shared, &reader, id, &body["response"], &note_of(&body)) {
                Ok(session) => {
                    followed.extend(session);
                    results.push(json!({ "id": id, "ok": true }));
                }
                Err(error) => results.push(json!({ "id": id, "ok": false, "error": error.told(shared.language()) })),
            }
        }
        Ok((results, followed))
    })
    .await?;
    for session in followed {
        ctx.follow(&session);
    }
    Ok(reply(results))
}
