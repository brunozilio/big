use std::path::PathBuf;

use axum::Json;
use axum::extract::State;
use axum::response::Response;
use serde_json::{Value, json};

use crate::error::{ApiError, ApiResult, said};
use crate::state::{Ctx, Shared};

pub const WORKFLOW: &str = "btw";

fn folder(shared: &Shared, body: &Value) -> ApiResult<PathBuf> {
    let id = body["application"].as_str().filter(|id| !id.is_empty());
    let id = id.ok_or_else(|| {
        ApiError::bad(said(
            "Choose an application to ask about.",
            "Elige una aplicación para preguntar.",
            "Escolha uma aplicação para perguntar.",
        ))
    })?;
    let application = shared.backend.store.application(id)?;
    let application = application.ok_or_else(|| {
        ApiError::bad(said(
            "That application no longer exists.",
            "Esa aplicación ya no existe.",
            "Essa aplicação não existe mais.",
        ))
    })?;
    let first = application.repositories.into_iter().map(|repository| PathBuf::from(repository.directory)).next();
    first.filter(|folder| folder.is_dir()).ok_or_else(|| {
        ApiError::bad(said(
            "That application has no code folder.",
            "Esa aplicación no tiene una carpeta de código.",
            "Essa aplicação não tem uma pasta de código.",
        ))
    })
}

pub async fn ask(State(ctx): State<Ctx>, Json(body): Json<Value>) -> ApiResult<Response> {
    let text = body["text"].as_str().map(str::trim).filter(|text| !text.is_empty());
    let text = text
        .ok_or_else(|| ApiError::bad(said("Write the question.", "Escribe la pregunta.", "Escreva a pergunta.")))?
        .to_owned();
    folder(&ctx, &body)?;
    let item = json!({
        "text": text, "application": body["application"], "model": body["model"], "workflow": WORKFLOW,
        "sources": [], "attachments": [],
    });
    crate::items::create(State(ctx), Json(item)).await
}
