use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;

use axum::Json;
use axum::extract::{Path as Route, State};
use axum::response::Response;
use domain::workflow::ItemId;
use platform::editor::{self, Editor};
use serde_json::Value;

use crate::error::{ApiError, ApiResult, done, said};
use crate::sessions::{blocking, folder, valid};
use crate::state::{Ctx, Shared};

fn no_folder() -> ApiError {
    ApiError::missing(said(
        "This item has no folder to open yet.",
        "Este ítem todavía no tiene una carpeta para abrir.",
        "Esta tarefa ainda não tem uma pasta para abrir.",
    ))
}

pub fn located(folder: &Path, wanted: &str) -> Option<PathBuf> {
    let wanted = wanted.trim();
    if wanted.is_empty() {
        return None;
    }
    let path = Path::new(wanted);
    let full = if path.is_absolute() { path.to_path_buf() } else { folder.join(path) };
    full.canonicalize().ok().filter(|found| found.is_file())
}

fn editor() -> ApiResult<Box<dyn Editor>> {
    editor::chosen(brand::editor::KIND, brand::editor::NAME, brand::editor::SCHEME).ok_or_else(|| {
        ApiError::new(
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            "failed",
            said(
                "The editor in the settings is not known.",
                "El editor de la configuración no es conocido.",
                "O editor da configuração não é conhecido.",
            ),
        )
    })
}

pub fn editor_name() -> &'static str {
    brand::editor::NAME
}

fn launch(url: &str) -> ApiResult<()> {
    let mut command = platform::open_url(url);
    command.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    let mut child = command.spawn().map_err(ApiError::failed)?;
    std::thread::spawn(move || child.wait());
    Ok(())
}

fn open_now(shared: &Shared, session: &str, body: &Value) -> ApiResult<()> {
    let folder = folder(shared, session)?;
    let wanted = body["path"].as_str().unwrap_or_default();
    let file = located(&folder, wanted).ok_or_else(|| {
        ApiError::missing(said(
            "That file is not in this session's folder.",
            "Ese archivo no está en la carpeta de esta sesión.",
            "Esse arquivo não está na pasta desta sessão.",
        ))
    })?;
    let line = body["line"].as_u64().unwrap_or(1);
    let column = body["column"].as_u64().unwrap_or(1);
    launch(&editor()?.file(&file, line, column))
}

fn open_item(shared: &Shared, id: &ItemId) -> ApiResult<()> {
    let found =
        shared.backend.orchestrator.folders(id)?.ok_or_else(|| {
            ApiError::missing(said("There is no such item.", "No existe ese ítem.", "Não há esse item."))
        })?;
    if found.folders.is_empty() {
        return Err(no_folder());
    }
    let url = editor()?.folders(&found.folders, &found.workspace).map_err(ApiError::failed)?;
    launch(&url)
}

pub async fn open_folders(State(ctx): State<Ctx>, Route(id): Route<String>) -> ApiResult<Response> {
    let id = ItemId::from(id.as_str());
    let shared = Arc::clone(&ctx);
    blocking(move || open_item(&shared, &id)).await?;
    Ok(done())
}

pub async fn open(State(ctx): State<Ctx>, Route(id): Route<String>, Json(body): Json<Value>) -> ApiResult<Response> {
    let id = valid(&id)?.to_owned();
    let shared = Arc::clone(&ctx);
    blocking(move || open_now(&shared, &id, &body)).await?;
    Ok(done())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_relative_path_is_found_inside_the_session_folder() {
        let dir = tempfile::tempdir().expect("dir");
        std::fs::create_dir(dir.path().join("test")).expect("mkdir");
        std::fs::write(dir.path().join("test/calc.test.js"), "").expect("write");
        let found = located(dir.path(), "test/calc.test.js").expect("found");
        assert!(found.ends_with("test/calc.test.js"));
        assert!(located(dir.path(), "test/missing.js").is_none());
        assert!(located(dir.path(), "test").is_none());
        assert!(located(dir.path(), " ").is_none());
    }
}
