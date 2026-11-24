use std::path::{Path, PathBuf};

use axum::Json;
use axum::extract::State;
use axum::extract::ws::WebSocketUpgrade;
use axum::http::HeaderMap;
use axum::http::header::ORIGIN;
use axum::response::Response;
use serde_json::{Value, json};

use crate::state::Ctx;

fn folder(data: &Path) -> PathBuf {
    data.join(brand::browser::EXTENSION_FOLDER)
}

pub fn install(data: &Path, port: u16) {
    match browser::install(&folder(data), port) {
        Ok(installed) if installed.written > 0 => {
            tracing::info!(folder = %installed.folder.display(), "the browser extension was written");
        }
        Ok(_) => {}
        Err(error) => tracing::warn!(%error, "the browser extension could not be written"),
    }
}

pub fn for_runs(hub: &browser::Hub, data: &Path) -> app::BrowserMcp {
    let hub = hub.clone();
    let settings = browser::Settings::configured(data);
    let shared = [data.join("attachments"), settings.files.clone()];
    app::BrowserMcp::new(move |session, mut roots, ask| {
        roots.extend(shared.iter().cloned());
        browser::Session::new(hub.clone(), session, settings.clone(), roots).asking(ask).local()
    })
}

pub async fn connect(State(ctx): State<Ctx>, headers: HeaderMap, upgrade: WebSocketUpgrade) -> Response {
    let origin = headers.get(ORIGIN).and_then(|value| value.to_str().ok());
    browser::connect(ctx.backend.browser.clone(), upgrade, origin)
}

pub async fn status(State(ctx): State<Ctx>) -> Json<Value> {
    Json(json!({
        "server": browser::server_name(),
        "browsers": ctx.backend.browser.browsers(),
        "extensionFolder": folder(&ctx.backend.data),
        "tools": browser::tools().iter().map(|tool| tool["name"].clone()).collect::<Vec<Value>>(),
    }))
}
