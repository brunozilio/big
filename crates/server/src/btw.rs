use std::path::PathBuf;
use std::sync::Arc;

use axum::Json;
use axum::extract::State;
use axum::response::Response;
use serde_json::{Value, json};

use crate::chats::{Notify, Opening};
use crate::error::{ApiError, ApiResult, json as reply};
use crate::state::{Ctx, Shared};

const MOST_TURNS: u32 = 40;

const BRIEF: &str = "This conversation was opened from a quick side question, outside any task. Look around the project \
as much as you need, but change nothing unless the person approves a plan: no edits, no commits, no commands that write. \
Answer directly and briefly, in the language of the question.";

fn folder(shared: &Shared, body: &Value) -> ApiResult<PathBuf> {
    let id = body["application"].as_str().filter(|id| !id.is_empty());
    let id = id.ok_or_else(|| ApiError::bad("Escolha uma aplicação para perguntar."))?;
    let application = shared.backend.store.application(id)?;
    let application = application.ok_or_else(|| ApiError::bad("Essa aplicação não existe mais."))?;
    let first = application.repositories.into_iter().map(|repository| PathBuf::from(repository.directory)).next();
    first.filter(|folder| folder.is_dir()).ok_or_else(|| ApiError::bad("Essa aplicação não tem uma pasta de código."))
}

fn open_now(ctx: &Ctx, body: &Value) -> ApiResult<Value> {
    let shared = ctx.as_ref();
    let text = body["text"].as_str().map(str::trim).filter(|text| !text.is_empty());
    let text = text.ok_or_else(|| ApiError::bad("Escreva a pergunta."))?.to_owned();
    let folder = folder(shared, body)?;
    let models = shared.backend.client.models().map_err(ApiError::failed)?;
    let model = domain::model::choose_model(&models, body["model"].as_str())
        .ok_or_else(|| ApiError::bad(crate::models::NONE))?;
    let nudged = Arc::clone(ctx);
    let notify: Notify = Arc::new(move |session: &str| nudged.follow(session));
    let opening = Opening { folder, text: &text, model, brief: BRIEF.to_owned(), most_turns: MOST_TURNS, style: shared.backend.store.output_style() };
    let session = shared.chats.open(&shared.backend.client, opening, notify)?;
    Ok(json!({ "session": session }))
}

pub async fn ask(State(ctx): State<Ctx>, Json(body): Json<Value>) -> ApiResult<Response> {
    let shared = Arc::clone(&ctx);
    let opened = tokio::task::spawn_blocking(move || open_now(&shared, &body)).await.map_err(ApiError::failed)??;
    Ok(reply(opened))
}
