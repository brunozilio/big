use std::path::{Path, PathBuf};

use axum::Router;
use axum::extract::{DefaultBodyLimit, State};
use axum::http::{StatusCode, Uri};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::{any, delete, get, post};
use tower_http::services::{ServeDir, ServeFile};

use crate::error::ApiError;
use crate::state::Ctx;
use crate::{attachments, branding, connections, items, models, sessions};

const MOST_BODY_BYTES: usize = 32 * 1024 * 1024;

fn api() -> Router<Ctx> {
    Router::new()
        .route("/api/items", get(items::list).post(items::create))
        .route("/api/items/{id}/rerun", post(items::rerun))
        .route("/api/items/{id}/cancel", post(items::cancel))
        .route("/api/items/{id}/steps/{step}", get(items::step))
        .route("/api/items/{id}/steps/{step}/input", post(items::human))
        .route("/api/sessions/{id}", get(sessions::open))
        .route("/api/sessions/{id}/messages", post(sessions::send))
        .route("/api/sessions/{id}/respond", post(sessions::respond))
        .route("/api/sessions/{id}/stop", post(sessions::stop))
        .route("/api/connections", get(connections::list))
        .route("/api/connections/oauth/callback", get(connections::callback))
        .route("/oauth/callback", get(connections::callback))
        .route("/api/connections/device/poll", post(connections::poll))
        .route("/api/connections/{id}", delete(connections::remove))
        .route("/api/connections/{id}/start", post(connections::start))
        .route("/api/connections/{id}/call", post(connections::call))
        .route("/api/models", get(models::list))
        .route("/api/branding", get(branding::get).put(branding::put))
        .route("/api/attachments", post(attachments::upload))
        .route("/api/live", get(live))
        .route("/api/health", get(|| async { axum::Json(serde_json::json!({ "status": "ok" })) }))
        .route("/api/{*rest}", any(|| async { ApiError::missing("Não há esse endereço.") }))
}

async fn live(State(ctx): State<Ctx>) -> Response {
    ctx.live.response()
}

fn chosen_folder() -> Option<PathBuf> {
    Some(PathBuf::from(brand::server::ASSETS_DIR)).filter(|folder| folder.join("index.html").is_file())
}

fn from_folder(api: Router<Ctx>, dist: &Path) -> Router<Ctx> {
    api.fallback_service(ServeDir::new(dist).fallback(ServeFile::new(dist.join("index.html"))))
}

pub fn router(ctx: Ctx) -> Router {
    let bundle = ctx.bundle;
    let api = api();
    let app = match chosen_folder() {
        Some(dist) => from_folder(api, &dist),
        None if page::has_index(bundle) => api.fallback(move |uri: Uri| async move { page::serve(bundle, &uri) }),
        None => api.fallback(|| async { (StatusCode::SERVICE_UNAVAILABLE, Html(page::MISSING)).into_response() }),
    };
    app.layer(DefaultBodyLimit::max(MOST_BODY_BYTES)).with_state(ctx)
}
