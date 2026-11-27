use std::path::{Path, PathBuf};
use std::sync::Arc;

use axum::Json;
use axum::extract::State;
use axum::extract::ws::WebSocketUpgrade;
use axum::http::HeaderMap;
use axum::http::header::ORIGIN;
use axum::response::Response;
use serde_json::{Value, json};
use store::Store;

use crate::error::{ApiError, ApiResult, json as reply, said};
use crate::state::Ctx;

const ENABLED: &str = "browser.enabled";

pub fn enabled(store: &Store) -> bool {
    store.preference(ENABLED).ok().flatten().and_then(|value| value.as_bool()).unwrap_or(true)
}

fn folder(data: &Path) -> PathBuf {
    data.join(brand::browser::EXTENSION_FOLDER)
}

pub fn install(hub: &browser::Hub, data: &Path, port: u16) {
    match browser::install(&folder(data), port) {
        Ok(installed) => {
            hub.expect(&installed.build);
            if installed.written > 0 {
                tracing::info!(folder = %installed.folder.display(), "the browser extension was written");
            }
        }
        Err(error) => tracing::warn!(%error, "the browser extension could not be written"),
    }
}

pub fn for_runs(hub: &browser::Hub, data: &Path, store: Arc<Store>) -> app::BrowserMcp {
    let hub = hub.clone();
    let settings = browser::Settings::configured(data);
    let shared = [data.join("attachments"), settings.files.clone()];
    app::BrowserMcp::new(move |session, mut roots, ask| {
        if !enabled(&store) {
            return None;
        }
        roots.extend(shared.iter().cloned());
        Some(browser::Session::new(hub.clone(), session, settings.clone(), roots).asking(ask).local())
    })
}

pub async fn connect(State(ctx): State<Ctx>, headers: HeaderMap, upgrade: WebSocketUpgrade) -> Response {
    let origin = headers.get(ORIGIN).and_then(|value| value.to_str().ok());
    browser::connect(ctx.backend.browser.clone(), upgrade, origin)
}

fn status_of(ctx: &Ctx) -> Value {
    json!({
        "enabled": enabled(&ctx.backend.store),
        "server": browser::server_name(),
        "browsers": ctx.backend.browser.browsers(),
        "extensionFolder": folder(&ctx.backend.data),
        "mcp": {
            "url": format!("http://127.0.0.1:{}{}", ctx.port, crate::mcp::route()),
            "tokenFile": crate::mcp::token_file(&ctx.backend.data),
            "headers": { crate::mcp::resources_header(): crate::mcp::BROWSER },
            "command": [
                std::env::current_exe().unwrap_or_default(),
                PathBuf::from(brand::mcp::COMMAND.trim()),
                PathBuf::from(crate::mcp::BROWSER),
            ],
        },
        "tools": browser::tools().iter().map(|tool| tool["name"].clone()).collect::<Vec<Value>>(),
    })
}

pub async fn status(State(ctx): State<Ctx>) -> Json<Value> {
    Json(status_of(&ctx))
}

pub async fn set(State(ctx): State<Ctx>, Json(patch): Json<Value>) -> ApiResult<Response> {
    let on = patch.get("enabled").and_then(Value::as_bool).ok_or_else(|| {
        ApiError::bad(said(
            "Send enabled as true or false.",
            "Envía enabled como true o false.",
            "Informe enabled como true ou false.",
        ))
    })?;
    ctx.backend.store.set_preference(ENABLED, &json!(on))?;
    Ok(reply(status_of(&ctx)))
}
