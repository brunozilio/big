use std::path::Path;

use axum::Json;
use axum::extract::{Path as UrlPath, State};
use axum::response::Response;
use serde_json::Value;
use store::{Application, CloudChoice, CodeChoice, Repository, TrackerChoice};

use crate::attachments::random_id;
use crate::error::{ApiError, ApiResult, done, json as reply, said};
use crate::state::{Ctx, Shared};

fn trimmed(text: &str) -> String {
    text.trim().to_owned()
}

fn lines(found: &[String]) -> Vec<String> {
    found.iter().map(|line| trimmed(line)).filter(|line| !line.is_empty()).collect()
}

fn repository(found: Repository) -> ApiResult<Repository> {
    let directory = trimmed(&found.directory);
    if directory.is_empty() {
        return Err(ApiError::bad(said(
            "Choose the folder of each repository.",
            "Elige la carpeta de cada repositorio.",
            "Escolha a pasta de cada repositório.",
        )));
    }
    if !Path::new(&directory).is_dir() {
        return Err(ApiError::bad(said(
            format!("The folder {directory} does not exist."),
            format!("La carpeta {directory} no existe."),
            format!("A pasta {directory} não existe."),
        )));
    }
    let id = Some(trimmed(&found.id)).filter(|id| !id.is_empty()).map_or_else(random_id, Ok)?;
    let code = found
        .code
        .map(|code| CodeChoice { connection: trimmed(&code.connection), repo: trimmed(&code.repo) })
        .filter(|code| !code.connection.is_empty() && !code.repo.is_empty());
    Ok(Repository { id, code, directory, run: lines(&found.run), verify: lines(&found.verify) })
}

fn cloud(chosen: CloudChoice) -> CloudChoice {
    CloudChoice {
        connection: trimmed(&chosen.connection),
        account: trimmed(&chosen.account),
        role: trimmed(&chosen.role),
    }
}

pub fn checked(mut application: Application) -> ApiResult<Application> {
    application.name = trimmed(&application.name);
    if application.name.is_empty() {
        return Err(ApiError::bad(said(
            "Give the application a name.",
            "Dale un nombre a la aplicación.",
            "Dê um nome à aplicação.",
        )));
    }
    if application.repositories.is_empty() {
        return Err(ApiError::bad(said(
            "Add at least one repository.",
            "Agrega al menos un repositorio.",
            "Adicione ao menos um repositório.",
        )));
    }
    application.tracker = application
        .tracker
        .map(|chosen| TrackerChoice { connection: trimmed(&chosen.connection), project: trimmed(&chosen.project) })
        .filter(|chosen| !chosen.connection.is_empty() && !chosen.project.is_empty());
    application.cloud =
        application.cloud.map(cloud).filter(|chosen| !chosen.connection.is_empty() && !chosen.account.is_empty());
    let mut kept: Vec<Repository> = Vec::new();
    for found in application.repositories {
        let mut found = repository(found)?;
        if kept.iter().any(|known| known.id == found.id) {
            found.id = random_id()?;
        }
        kept.push(found);
    }
    application.repositories = kept;
    Ok(application)
}

fn parsed(body: Value) -> ApiResult<Application> {
    serde_json::from_value(body).map_err(|error| {
        ApiError::bad(said(
            format!("The application came incomplete: {error}"),
            format!("La aplicación llegó incompleta: {error}"),
            format!("A aplicação veio incompleta: {error}"),
        ))
    })
}

pub async fn all(shared: &Shared) -> ApiResult<Vec<Application>> {
    Ok(shared.backend.store.applications()?)
}

pub fn snapshot(shared: &Shared, application: &Application) -> ApiResult<Value> {
    Ok(orchestrator::application_snapshot(&shared.backend.store, application))
}

pub async fn list(State(ctx): State<Ctx>) -> ApiResult<Response> {
    Ok(reply(all(&ctx).await?))
}

pub async fn create(State(ctx): State<Ctx>, Json(body): Json<Value>) -> ApiResult<Response> {
    let mut application = parsed(body)?;
    application.id = random_id()?;
    let application = checked(application)?;
    ctx.backend.store.save_application(&application)?;
    Ok(reply(application))
}

pub async fn update(
    State(ctx): State<Ctx>,
    UrlPath(id): UrlPath<String>,
    Json(body): Json<Value>,
) -> ApiResult<Response> {
    if ctx.backend.store.application(&id)?.is_none() {
        return Err(ApiError::missing(said(
            "There is no such application.",
            "No existe esa aplicación.",
            "Não há essa aplicação.",
        )));
    }
    let mut application = parsed(body)?;
    application.id = id;
    let application = checked(application)?;
    ctx.backend.store.save_application(&application)?;
    Ok(reply(application))
}

pub async fn remove(State(ctx): State<Ctx>, UrlPath(id): UrlPath<String>) -> ApiResult<Response> {
    if !ctx.backend.store.delete_application(&id)? {
        return Err(ApiError::missing(said(
            "There is no such application.",
            "No existe esa aplicación.",
            "Não há essa aplicação.",
        )));
    }
    Ok(done())
}
