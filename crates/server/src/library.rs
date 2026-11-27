use std::path::PathBuf;
use std::sync::Arc;

use axum::Json;
use axum::extract::{Query, State};
use axum::response::Response;
use orchestrator::library::{self, Change, Failure, Talk};
use serde::Deserialize;
use serde_json::Value;

use crate::error::{ApiError, ApiResult, done, json as reply, said};
use crate::state::{Ctx, Shared};

#[derive(Deserialize)]
pub struct Named {
    path: String,
}

#[derive(Deserialize)]
pub struct Saved {
    path: String,
    source: Option<String>,
    #[serde(default)]
    values: Value,
    body: Option<String>,
}

#[derive(Deserialize)]
pub struct Created {
    section: String,
    name: String,
}

fn refused(failure: Failure) -> ApiError {
    match failure {
        Failure::Missing => {
            ApiError::missing(said("There is no such item.", "No existe ese elemento.", "Esse item não existe."))
        }
        Failure::ReadOnly => ApiError::bad(said(
            "This installation keeps no content folder, so nothing can be changed here.",
            "Esta instalación no guarda una carpeta de contenido, así que aquí no se puede cambiar nada.",
            "Esta instalação não guarda uma pasta de conteúdo, então nada pode ser alterado aqui.",
        )),
        Failure::Exists => ApiError::bad(said(
            "There is already an item with that name.",
            "Ya existe un elemento con ese nombre.",
            "Já existe um item com esse nome.",
        )),
        Failure::Invalid(detail) => ApiError::bad(said(
            format!("It was not saved: {detail}"),
            format!("No se guardó: {detail}"),
            format!("Não foi salvo: {detail}"),
        )),
        Failure::Failed(detail) => ApiError::failed(detail),
    }
}

async fn worked<T: Send + 'static>(work: impl FnOnce() -> Result<T, Failure> + Send + 'static) -> ApiResult<T> {
    tokio::task::spawn_blocking(work).await.map_err(ApiError::failed)?.map_err(refused)
}

pub async fn list(State(ctx): State<Ctx>) -> ApiResult<Response> {
    let language = ctx.language().code();
    Ok(reply(worked(move || library::listing(language)).await?))
}

pub async fn open(State(ctx): State<Ctx>, Query(named): Query<Named>) -> ApiResult<Response> {
    let language = ctx.language().code();
    Ok(reply(worked(move || library::open(&named.path, language)).await?))
}

pub async fn save(State(ctx): State<Ctx>, Json(saved): Json<Saved>) -> ApiResult<Response> {
    let language = ctx.language().code();
    let change = Change { source: saved.source, values: saved.values, body: saved.body };
    Ok(reply(worked(move || library::save(&saved.path, &change, language)).await?))
}

pub async fn create(State(ctx): State<Ctx>, Json(created): Json<Created>) -> ApiResult<Response> {
    let language = ctx.language().code();
    let name = created.name.trim().to_owned();
    Ok(reply(worked(move || library::create(&created.section, &name, language)).await?))
}

pub async fn restore(State(ctx): State<Ctx>, Query(named): Query<Named>) -> ApiResult<Response> {
    let language = ctx.language().code();
    worked(move || library::restore(&named.path, language)).await?;
    Ok(done())
}

#[derive(Deserialize)]
pub struct Talked {
    path: String,
    #[serde(default)]
    values: Value,
    body: Option<String>,
    source: Option<String>,
    #[serde(default)]
    messages: Vec<Value>,
}

fn answered(shared: &Shared, talked: Talked) -> ApiResult<Value> {
    let asks = talked.messages.last().is_some_and(|last| {
        last["role"] == "user" && last["text"].as_str().is_some_and(|text| !text.trim().is_empty())
    });
    if !asks {
        return Err(ApiError::bad(said("Write what you want.", "Escribe lo que quieres.", "Escreva o que você quer.")));
    }
    let language = shared.language().code();
    let talk =
        Talk { values: talked.values, body: talked.body, source: talked.source, messages: Value::Array(talked.messages) };
    let prompt = library::assist(&talked.path, &talk, language).map_err(refused)?;
    let models = shared.backend.client.models().map_err(ApiError::failed)?;
    let model = domain::model::choose_model(&models, None).ok_or_else(|| ApiError::bad(crate::models::none()))?;
    let session = shared.backend.client.session(PathBuf::from("."), model);
    let answer = session.answer(&prompt).and_then(app::Run::answer_text).map_err(ApiError::failed)?;
    library::assisted(&talked.path, &talk, &answer, language).map_err(refused)
}

pub async fn assist(State(ctx): State<Ctx>, Json(talked): Json<Talked>) -> ApiResult<Response> {
    let shared = Arc::clone(&ctx);
    let done = tokio::task::spawn_blocking(move || answered(&shared, talked)).await.map_err(ApiError::failed)??;
    Ok(reply(done))
}
