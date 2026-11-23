use std::collections::HashMap;
use std::path::{Path, PathBuf};

use axum::extract::Query;
use axum::response::Response;
use serde_json::{Value, json};

use crate::error::{ApiError, ApiResult, json as reply};

pub fn repo_ref(url: &str) -> Option<String> {
    let url = url.trim().trim_end_matches('/');
    let url = url.strip_suffix(".git").unwrap_or(url);
    let path = match url.split_once("://") {
        Some((_, rest)) => rest.split_once('/')?.1,
        None => url.split_once(':')?.1,
    };
    let path = path.trim_matches('/');
    let parts: Vec<&str> = path.split('/').filter(|part| !part.is_empty()).collect();
    (parts.len() >= 2).then(|| parts.join("/"))
}

pub fn remote(dir: &Path) -> Value {
    let url = orchestrator::origin(dir);
    json!({ "url": url, "ref": url.as_deref().and_then(repo_ref) })
}

fn is_repo(dir: &Path) -> bool {
    dir.join(".git").exists()
}

fn entries(dir: &Path) -> ApiResult<Vec<Value>> {
    let listed =
        std::fs::read_dir(dir).map_err(|error| ApiError::bad(format!("Não foi possível abrir a pasta: {error}")))?;
    let mut found: Vec<(String, PathBuf)> = listed
        .flatten()
        .filter(|entry| entry.path().is_dir())
        .map(|entry| (entry.file_name().to_string_lossy().into_owned(), entry.path()))
        .filter(|(name, _)| !name.starts_with('.'))
        .collect();
    found.sort_by_key(|(name, _)| name.to_lowercase());
    Ok(found
        .into_iter()
        .map(|(name, path)| json!({ "name": name, "path": path.to_string_lossy(), "repo": is_repo(&path) }))
        .collect())
}

pub fn listing(path: Option<&str>) -> ApiResult<Value> {
    let home = std::env::home_dir().unwrap_or_else(|| PathBuf::from("/"));
    let wanted = path.map(str::trim).filter(|path| !path.is_empty()).map_or_else(|| home.clone(), PathBuf::from);
    if !wanted.is_absolute() {
        return Err(ApiError::bad("O caminho precisa ser absoluto."));
    }
    let dir = std::fs::canonicalize(&wanted).map_err(|_| ApiError::missing("Essa pasta não existe."))?;
    if !dir.is_dir() {
        return Err(ApiError::bad("Isso não é uma pasta."));
    }
    let repo = is_repo(&dir);
    Ok(json!({
        "path": dir.to_string_lossy(),
        "parent": dir.parent().map(|parent| parent.to_string_lossy().into_owned()),
        "home": home.to_string_lossy(),
        "repo": repo,
        "remote": if repo { remote(&dir) } else { json!({ "url": null, "ref": null }) },
        "entries": entries(&dir)?,
    }))
}

pub async fn dirs(Query(query): Query<HashMap<String, String>>) -> ApiResult<Response> {
    let path = query.get("path").cloned();
    let listed = tokio::task::spawn_blocking(move || listing(path.as_deref())).await.map_err(ApiError::failed)??;
    Ok(reply(listed))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_remote_becomes_owner_and_name() {
        assert_eq!(repo_ref("git@code.test:acme/web.git").as_deref(), Some("acme/web"));
        assert_eq!(repo_ref("https://code.test/acme/web").as_deref(), Some("acme/web"));
        assert_eq!(repo_ref("ssh://git@code.test:22/acme/web.git/").as_deref(), Some("acme/web"));
        assert_eq!(repo_ref("https://code.test/acme/group/web.git").as_deref(), Some("acme/group/web"));
        assert_eq!(repo_ref("/local/path"), None);
    }
}
