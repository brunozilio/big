use std::path::Path;

use axum::extract::{Path as UrlPath, State};
use axum::response::Response;
use axum::{Extension, Json};
use domain::principal::Principal;
use serde_json::Value;
use store::{Application, Binding, Bindings, Choice, Environment, Fields, Repository};

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
    let e2e_directory = inside(&found.e2e_directory)?;
    Ok(Repository {
        id,
        directory,
        kind: trimmed(&found.kind),
        run: lines(&found.run),
        verify: lines(&found.verify),
        e2e: lines(&found.e2e),
        e2e_directory,
        bindings: bound(found.bindings),
        flow: trimmed(&found.flow),
        read_only: found.read_only,
        remote: trimmed(&found.remote),
    })
}

fn inside(folder: &str) -> ApiResult<String> {
    let folder = trimmed(folder).trim_matches('/').to_owned();
    let path = Path::new(&folder);
    if path.components().all(|part| matches!(part, std::path::Component::Normal(_))) {
        return Ok(folder);
    }
    Err(ApiError::bad(said(
        format!("The E2E folder {folder} must be a path inside the repository."),
        format!("La carpeta E2E {folder} debe ser una ruta dentro del repositorio."),
        format!("A pasta do E2E {folder} precisa ser um caminho dentro do repositório."),
    )))
}

fn variable_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars.next().is_some_and(|first| first.is_ascii_alphabetic() || first == '_')
        && chars.all(|rest| rest.is_ascii_alphanumeric() || rest == '_')
}

fn environment(given: Environment) -> ApiResult<Environment> {
    let mut kept = Environment::new();
    for (name, value) in given {
        let name = trimmed(&name);
        if name.is_empty() {
            continue;
        }
        if !variable_name(&name) {
            return Err(ApiError::bad(said(
                format!("{name} is not a valid environment variable name: use letters, digits and _."),
                format!("{name} no es un nombre válido de variable de entorno: usa letras, dígitos y _."),
                format!("{name} não é um nome válido de variável de ambiente: use letras, dígitos e _."),
            )));
        }
        kept.insert(name, value);
    }
    Ok(kept)
}

fn chosen(given: Fields) -> Fields {
    given
        .into_iter()
        .map(|(key, choice)| (trimmed(&key), Choice { value: trimmed(&choice.value), label: trimmed(&choice.label) }))
        .filter(|(key, choice)| !key.is_empty() && !choice.value.is_empty())
        .collect()
}

fn bound(given: Bindings) -> Bindings {
    given
        .into_iter()
        .map(|(capability, binding)| {
            let mut more: Vec<Choice> = Vec::new();
            for choice in binding.more {
                let value = trimmed(&choice.value);
                if !value.is_empty() && value != binding.value.trim() && !more.iter().any(|known| known.value == value)
                {
                    more.push(Choice { value, label: trimmed(&choice.label) });
                }
            }
            let profiles: Vec<Fields> =
                binding.profiles.into_iter().map(chosen).filter(|profile| !profile.is_empty()).collect();
            let binding = Binding {
                connection: trimmed(&binding.connection),
                value: trimmed(&binding.value),
                label: trimmed(&binding.label),
                more,
                fields: chosen(binding.fields),
                profiles,
            };
            (trimmed(&capability), binding)
        })
        .filter(|(capability, binding)| !capability.is_empty() && binding.is_set())
        .collect()
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
    application.bindings = bound(application.bindings);
    application.env = environment(application.env)?;
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

pub async fn all(shared: &Shared, owner: &str) -> ApiResult<Vec<Application>> {
    let theirs = shared.backend.store.owned_ids(store::ownership::APPLICATION, owner)?;
    Ok(shared
        .backend
        .store
        .applications()?
        .into_iter()
        .filter(|application| theirs.contains(&application.id))
        .collect())
}

pub fn snapshot(shared: &Shared, application: &Application) -> ApiResult<Value> {
    Ok(orchestrator::application_snapshot(&shared.backend.store, application))
}

pub async fn list(State(ctx): State<Ctx>, Extension(person): Extension<Principal>) -> ApiResult<Response> {
    Ok(reply(all(&ctx, person.owner()).await?))
}

pub async fn create(
    State(ctx): State<Ctx>,
    Extension(person): Extension<Principal>,
    Json(body): Json<Value>,
) -> ApiResult<Response> {
    let mut application = parsed(body)?;
    application.id = random_id()?;
    let application = crate::workspaces::cloned(&ctx, person.owner(), application).await?;
    let application = checked(application)?;
    ctx.backend.store.save_application(&application)?;
    ctx.backend.store.set_owner(store::ownership::APPLICATION, &application.id, person.owner())?;
    Ok(reply(application))
}

pub async fn update(
    State(ctx): State<Ctx>,
    UrlPath(id): UrlPath<String>,
    Extension(person): Extension<Principal>,
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
    let application = crate::workspaces::cloned(&ctx, person.owner(), application).await?;
    let application = checked(application)?;
    ctx.backend.store.save_application(&application)?;
    Ok(reply(application))
}

pub async fn remove(State(ctx): State<Ctx>, UrlPath(id): UrlPath<String>) -> ApiResult<Response> {
    ctx.backend.store.forget_owner(store::ownership::APPLICATION, &id)?;
    if !ctx.backend.store.delete_application(&id)? {
        return Err(ApiError::missing(said(
            "There is no such application.",
            "No existe esa aplicación.",
            "Não há essa aplicação.",
        )));
    }
    Ok(done())
}
