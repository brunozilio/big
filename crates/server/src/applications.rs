use std::path::Path;

use axum::Json;
use axum::extract::{Path as UrlPath, State};
use axum::response::Response;
use connectors::contracts::{self, code, tracker};
use serde_json::{Value, json};
use store::{Application, CodeChoice, ConnectionRecord, Repository, TrackerChoice};

use crate::attachments::random_id;
use crate::error::{ApiError, ApiResult, done, json as reply};
use crate::state::{Ctx, Shared};

fn trimmed(text: &str) -> String {
    text.trim().to_owned()
}

fn repository(found: Repository) -> ApiResult<Repository> {
    let directory = trimmed(&found.directory);
    if directory.is_empty() {
        return Err(ApiError::bad("Escolha a pasta de cada repositório."));
    }
    if !Path::new(&directory).is_dir() {
        return Err(ApiError::bad(format!("A pasta {directory} não existe.")));
    }
    let id = Some(trimmed(&found.id)).filter(|id| !id.is_empty()).map_or_else(random_id, Ok)?;
    let code = found
        .code
        .map(|code| CodeChoice { connection: trimmed(&code.connection), repo: trimmed(&code.repo) })
        .filter(|code| !code.connection.is_empty() && !code.repo.is_empty());
    let verify = found.verify.iter().map(|line| trimmed(line)).filter(|line| !line.is_empty()).collect();
    Ok(Repository { id, code, directory, run: trimmed(&found.run), verify })
}

pub fn checked(mut application: Application) -> ApiResult<Application> {
    application.name = trimmed(&application.name);
    if application.name.is_empty() {
        return Err(ApiError::bad("Dê um nome à aplicação."));
    }
    if application.repositories.is_empty() {
        return Err(ApiError::bad("Adicione ao menos um repositório."));
    }
    application.tracker = application
        .tracker
        .map(|chosen| TrackerChoice { connection: trimmed(&chosen.connection), project: trimmed(&chosen.project) })
        .filter(|chosen| !chosen.connection.is_empty() && !chosen.project.is_empty());
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
    serde_json::from_value(body).map_err(|error| ApiError::bad(format!("A aplicação veio incompleta: {error}")))
}

fn with(records: &[ConnectionRecord], capability: &str) -> Vec<ConnectionRecord> {
    records.iter().filter(|record| record.capabilities.iter().any(|name| name == capability)).cloned().collect()
}

async fn guessed_code(shared: &Shared, records: &[ConnectionRecord], dir: &Path) -> Option<CodeChoice> {
    let place = dir.to_path_buf();
    let remote = tokio::task::spawn_blocking(move || crate::folders::remote(&place)).await.ok()?;
    let wanted = remote["ref"].as_str()?.to_owned();
    let name = wanted.rsplit('/').next().unwrap_or_default().to_owned();
    for record in with(records, contracts::CODE) {
        let Ok(listed) = crate::calls::action(shared, &record, code::REPOS, json!({ "query": name })).await else {
            continue;
        };
        let found = listed.as_array().into_iter().flatten().find_map(|repo| {
            repo["ref"].as_str().filter(|found| found.eq_ignore_ascii_case(&wanted)).map(str::to_owned)
        });
        if let Some(repo) = found {
            return Some(CodeChoice { connection: record.id, repo });
        }
    }
    None
}

async fn guessed_tracker(shared: &Shared, records: &[ConnectionRecord]) -> Option<TrackerChoice> {
    let [record] = with(records, contracts::TRACKER).try_into().ok()?;
    let listed = crate::calls::action(shared, &record, tracker::PROJECTS, json!({})).await.ok()?;
    let [project] = listed.as_array()?.as_slice() else { return None };
    let key = project["key"].as_str().filter(|key| !key.is_empty())?;
    Some(TrackerChoice { connection: record.id, project: key.to_owned() })
}

pub async fn first(shared: &Shared) -> ApiResult<Option<Application>> {
    let store = &shared.backend.store;
    if store.applications_seeded()? {
        return Ok(None);
    }
    let Some(dir) = shared.backend.orchestrator.directory() else {
        store.seed_applications(&[])?;
        return Ok(None);
    };
    let records = store.connections()?;
    let name =
        dir.file_name().map_or_else(|| dir.to_string_lossy().into_owned(), |name| name.to_string_lossy().into_owned());
    let repository = Repository {
        id: random_id()?,
        code: guessed_code(shared, &records, &dir).await,
        directory: dir.to_string_lossy().into_owned(),
        run: String::new(),
        verify: Vec::new(),
    };
    let tracker = guessed_tracker(shared, &records).await;
    let application = Application { id: random_id()?, name, tracker, repositories: vec![repository] };
    Ok(store.seed_applications(std::slice::from_ref(&application))?.then_some(application))
}

pub async fn all(shared: &Shared) -> ApiResult<Vec<Application>> {
    first(shared).await?;
    Ok(shared.backend.store.applications()?)
}

pub fn snapshot(shared: &Shared, application: &Application) -> ApiResult<Value> {
    let records = shared.backend.store.connections()?;
    let mut value = serde_json::to_value(application).map_err(ApiError::failed)?;
    for repository in value["repositories"].as_array_mut().into_iter().flatten() {
        let Some(connection) = repository["code"]["connection"].as_str().map(str::to_owned) else { continue };
        let record = records.iter().find(|record| record.id == connection);
        let pipelines =
            record.is_some_and(|record| record.capabilities.iter().any(|name| name == contracts::PIPELINES));
        repository["code"]["pipelines"] = json!(pipelines);
    }
    Ok(value)
}

pub async fn list(State(ctx): State<Ctx>) -> ApiResult<Response> {
    Ok(reply(all(&ctx).await?))
}

pub async fn create(State(ctx): State<Ctx>, Json(body): Json<Value>) -> ApiResult<Response> {
    first(&ctx).await?;
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
        return Err(ApiError::missing("Não há essa aplicação."));
    }
    let mut application = parsed(body)?;
    application.id = id;
    let application = checked(application)?;
    ctx.backend.store.save_application(&application)?;
    Ok(reply(application))
}

pub async fn remove(State(ctx): State<Ctx>, UrlPath(id): UrlPath<String>) -> ApiResult<Response> {
    if !ctx.backend.store.delete_application(&id)? {
        return Err(ApiError::missing("Não há essa aplicação."));
    }
    Ok(done())
}
