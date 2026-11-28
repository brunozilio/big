use std::collections::HashMap;
use std::path::{Path, PathBuf};

use axum::extract::{Path as UrlPath, Query, State};
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use connectors::contracts::{self, code};
use serde_json::json;
use store::{Application, Repository};

use crate::attachments::random_id;
use crate::error::{ApiError, ApiResult, said};
use crate::state::{Ctx, Shared};

pub const CREDENTIALS_PATH: &str = "/api/git/credentials/";
pub const HELPER_COMMAND: &str = "git-credential";

pub fn route() -> String {
    format!("{CREDENTIALS_PATH}{{token}}")
}

pub fn root(shared: &Shared, owner: &str) -> PathBuf {
    shared.backend.data.join("workspaces").join(owner)
}

fn wants_clone(repository: &Repository) -> bool {
    !repository.remote.trim().is_empty()
        && (repository.directory.trim().is_empty() || !Path::new(&repository.directory).is_dir())
}

pub fn https_only(remote: &str) -> ApiResult<()> {
    let fine = url::Url::parse(remote).is_ok_and(|url| url.scheme() == "https" && url.host_str().is_some());
    if fine {
        return Ok(());
    }
    Err(ApiError::bad(said(
        "The repository address must start with https://.",
        "La dirección del repositorio debe empezar con https://.",
        "O endereço do repositório precisa começar com https://.",
    )))
}

fn code_connection(shared: &Shared, owner: &str, application: &Application, repository: &Repository) -> Option<String> {
    let bound = repository.bound(contracts::CODE).or_else(|| application.bound(contracts::CODE));
    if let Some(binding) = bound {
        return Some(binding.connection.clone());
    }
    let theirs = shared.backend.store.connections_of(owner).ok()?;
    theirs.into_iter().find(|record| record.capabilities.iter().any(|name| name == contracts::CODE)).map(|r| r.id)
}

fn helper(shared: &Shared, owner: &str, connection: Option<&str>) -> ApiResult<String> {
    let Some(connection) = connection else { return Ok(String::new()) };
    let token = format!("{}{}", random_id()?, random_id()?);
    shared.backend.store.put_git_token(&token, owner, connection)?;
    let exe = std::env::current_exe().map_err(ApiError::failed)?;
    let url = format!("http://127.0.0.1:{}{CREDENTIALS_PATH}{token}", shared.port);
    Ok(format!("!\"{}\" {HELPER_COMMAND} \"{url}\"", exe.display()))
}

fn clone(remote: &str, folder: &Path, helper: &str, owner: &str) -> ApiResult<()> {
    let parent = folder.parent().ok_or_else(|| ApiError::failed("the workspace has no parent folder"))?;
    platform::confine::dir_for(parent, Some(owner)).map_err(ApiError::failed)?;
    let mut command = platform::git::command(parent);
    if !helper.is_empty() {
        command.args(["-c", &format!("credential.helper={helper}")]);
    }
    let done = command.args(["clone", "--quiet", remote]).arg(folder).output().map_err(ApiError::failed)?;
    if !done.status.success() {
        let told = String::from_utf8_lossy(&done.stderr).trim().to_owned();
        return Err(ApiError::bad(said(
            format!("The repository could not be cloned: {told}"),
            format!("No se pudo clonar el repositorio: {told}"),
            format!("Não foi possível clonar o repositório: {told}"),
        )));
    }
    if !helper.is_empty() {
        let kept = platform::git::command(folder).args(["config", "credential.helper", helper]).output();
        kept.map_err(ApiError::failed)?;
    }
    Ok(())
}

pub async fn cloned(shared: &Shared, owner: &str, mut application: Application) -> ApiResult<Application> {
    let base = root(shared, owner);
    let mut work = Vec::new();
    let found = application.clone();
    for repository in application.repositories.iter_mut().filter(|repository| wants_clone(repository)) {
        https_only(repository.remote.trim())?;
        if repository.id.trim().is_empty() {
            repository.id = random_id()?;
        }
        let folder = base.join(&repository.id);
        let connection = code_connection(shared, owner, &found, repository);
        let helper = helper(shared, owner, connection.as_deref())?;
        repository.directory = folder.to_string_lossy().into_owned();
        if !folder.join(".git").exists() {
            work.push((repository.remote.trim().to_owned(), folder, helper));
        }
    }
    for (remote, folder, helper) in work {
        let owner = owner.to_owned();
        let cloning = move || clone(&remote, &folder, &helper, &owner);
        tokio::task::spawn_blocking(cloning).await.map_err(ApiError::failed)??;
    }
    Ok(application)
}

pub async fn credentials(
    State(ctx): State<Ctx>,
    UrlPath(token): UrlPath<String>,
    Query(query): Query<HashMap<String, String>>,
) -> ApiResult<Response> {
    let missing = || ApiError::missing(said("Unknown token.", "Token desconocido.", "Token desconhecido."));
    let (owner, connection) = ctx.backend.store.git_token(&token)?.ok_or_else(missing)?;
    let record = ctx.backend.store.connection(&connection)?.ok_or_else(missing)?;
    if ctx.backend.store.owner_of(store::ownership::CONNECTION, &record.id)? != owner {
        return Err(missing());
    }
    let url = query.get("url").cloned().unwrap_or_default();
    let found = crate::calls::run(&ctx, &record, code::GIT_CREDENTIALS, &json!({ "url": url })).await?;
    let (user, password) =
        (found["username"].as_str().unwrap_or_default(), found["password"].as_str().unwrap_or_default());
    if user.is_empty() || password.is_empty() {
        return Ok((StatusCode::NOT_FOUND, "").into_response());
    }
    let text = format!("username={user}\npassword={password}\n");
    Ok(([(header::CONTENT_TYPE, "text/plain; charset=utf-8")], text).into_response())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_https_repositories_are_cloned_by_the_server() {
        assert!(https_only("https://github.com/acme/loja.git").is_ok());
        assert!(https_only("file:///etc").is_err());
        assert!(https_only("git@github.com:acme/loja.git").is_err());
        assert!(https_only("http://github.com/acme/loja").is_err());
    }

    #[test]
    fn a_repository_with_a_remote_and_no_folder_is_cloned() {
        let mut repository = Repository { remote: "https://x/y".into(), ..Repository::default() };
        assert!(wants_clone(&repository));
        repository.remote.clear();
        assert!(!wants_clone(&repository));
    }
}
