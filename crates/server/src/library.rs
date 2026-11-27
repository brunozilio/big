use std::convert::Infallible;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::Json;
use axum::extract::{Query, State};
use axum::response::sse::{Event, Sse};
use axum::response::{IntoResponse, Response};
use domain::{ChoreAnswer, EngineEvent};
use futures_util::{StreamExt, stream};
use orchestrator::library::{self, Change, Failure, Talk};
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::sync::mpsc;

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
    section: Option<String>,
    item: Option<String>,
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
    let made = worked(move || match (created.item, created.section) {
        (Some(item), _) => library::create_file(&item, &name, language),
        (None, Some(section)) => library::create(&section, &name, language),
        (None, None) => Err(Failure::Missing),
    });
    Ok(reply(made.await?))
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
    model: Option<String>,
}

struct Started {
    run: app::Run,
    path: String,
    talk: Talk,
    language: &'static str,
}

const PACE: Duration = Duration::from_millis(90);

fn started(shared: &Shared, talked: Talked) -> ApiResult<Started> {
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
    let model = domain::model::choose_model(&models, talked.model.as_deref())
        .ok_or_else(|| ApiError::bad(crate::models::none()))?;
    let session = shared.backend.client.session(PathBuf::from("."), model);
    let run = session.answer(&prompt).map_err(ApiError::failed)?;
    Ok(Started { run, path: talked.path, talk, language })
}

fn finished(started: &Started, answer: Option<String>, failure: Option<String>) -> Value {
    let done = match (answer, failure) {
        (Some(answer), _) => library::assisted(&started.path, &started.talk, &answer, started.language)
            .map_err(|failure| format!("{failure:?}")),
        (None, failure) => Err(failure.unwrap_or_else(|| "the request got no answer".to_owned())),
    };
    match done {
        Ok(mut done) => {
            done["type"] = json!("done");
            done
        }
        Err(message) => json!({ "type": "error", "message": message }),
    }
}

fn followed(mut started: Started, told: &mpsc::UnboundedSender<Value>) {
    let mut heard = String::new();
    let mut shown = String::new();
    let mut last = Instant::now();
    let mut answer = None;
    let mut failure = None;
    while let Some(event) = started.run.next_event() {
        match event {
            EngineEvent::TextDelta(piece) => {
                heard.push_str(&piece);
                if last.elapsed() < PACE {
                    continue;
                }
                last = Instant::now();
                let saying = library::saying(&heard);
                if saying != shown && told.send(json!({ "type": "reply", "text": saying })).is_err() {
                    return;
                }
                shown = saying;
            }
            EngineEvent::Chore(ChoreAnswer::Answer(text)) => answer = Some(text),
            EngineEvent::Failed(message) => failure = Some(message),
            EngineEvent::Cancelled => failure = Some("cancelled".to_owned()),
            _ => {}
        }
    }
    let _ = told.send(finished(&started, answer, failure));
}

pub async fn assist(State(ctx): State<Ctx>, Json(talked): Json<Talked>) -> ApiResult<Response> {
    let shared = Arc::clone(&ctx);
    let started = tokio::task::spawn_blocking(move || started(&shared, talked)).await.map_err(ApiError::failed)??;
    let (told, heard) = mpsc::unbounded_channel();
    tokio::task::spawn_blocking(move || followed(started, &told));
    let events = stream::unfold(heard, |mut heard| async move { heard.recv().await.map(|event| (event, heard)) })
        .map(|event: Value| Ok::<Event, Infallible>(Event::default().data(event.to_string())));
    Ok(Sse::new(events).into_response())
}
