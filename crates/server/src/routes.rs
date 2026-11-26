use std::path::{Path, PathBuf};

use axum::Router;
use axum::extract::{DefaultBodyLimit, State};
use axum::http::{StatusCode, Uri};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::{any, delete, get, post, put};
use tower_http::services::{ServeDir, ServeFile};

use crate::error::{ApiError, said};
use crate::state::Ctx;
use crate::{applications, attachments, branding, cloud, connections, folders, items, models, sessions, usage};

const MOST_BODY_BYTES: usize = 32 * 1024 * 1024;

fn api() -> Router<Ctx> {
    Router::new()
        .route("/api/items", get(items::list).post(items::create))
        .route("/api/items/{id}/rerun", post(items::rerun))
        .route("/api/items/{id}/cancel", post(items::cancel))
        .route("/api/items/{id}/open", post(crate::editor::open_folders))
        .route("/api/items/{id}/usage", get(crate::item_usage::usage))
        .route("/api/items/{id}/archive", post(crate::archive::archive).delete(crate::archive::unarchive))
        .route("/api/items/{id}/steps/{step}", get(items::step))
        .route("/api/items/{id}/steps/{step}/input", post(items::human))
        .route("/api/sessions/{id}", get(sessions::open))
        .route("/api/sessions/{id}/messages", post(sessions::send))
        .route("/api/sessions/{id}/respond", post(sessions::respond))
        .route("/api/sessions/{id}/stop", post(sessions::stop))
        .route("/api/sessions/{id}/jobs/{job}/stop", post(sessions::stop_job))
        .route("/api/sessions/{id}/side", post(sessions::side))
        .route("/api/sessions/{id}/open", post(crate::editor::open))
        .route("/api/btw", post(crate::btw::ask))
        .route("/api/connections", get(connections::list))
        .route("/api/connections/oauth/callback", get(connections::callback))
        .route("/oauth/callback", get(connections::callback))
        .route("/api/connections/device/poll", post(connections::poll))
        .route("/api/connections/{id}", delete(connections::remove))
        .route("/api/connections/{id}/start", post(connections::start))
        .route("/api/connections/{id}/call", post(connections::call))
        .route("/api/models", get(models::list))
        .route("/api/flows", get(models::flows))
        .route("/api/usage/limits", get(crate::limits::list))
        .route("/api/usage", get(usage::report))
        .route("/api/branding", get(branding::get).put(branding::put))
        .route(branding::LOGO_ROUTE, get(branding::logo))
        .route(branding::FAVICON_ROUTE, get(branding::favicon))
        .route("/api/applications", get(applications::list).post(applications::create))
        .route("/api/applications/{id}", put(applications::update).delete(applications::remove))
        .route(&cloud::route(), get(cloud::credentials))
        .route("/api/fs/dirs", get(folders::dirs))
        .route("/api/attachments", post(attachments::upload))
        .route("/api/attachments/{id}", get(attachments::download))
        .route("/api/artifacts/{id}", get(crate::artifacts::meta))
        .route("/api/artifacts/{id}/content", get(crate::artifacts::content))
        .route("/api/permissions", get(crate::permissions::get).put(crate::permissions::put))
        .route(
            "/api/permissions/discover",
            get(crate::discover::status).post(crate::discover::discover).delete(crate::discover::dismiss),
        )
        .route("/api/memories", get(crate::memories::list).post(crate::memories::edit))
        .route("/api/memories/move", post(crate::memories::moved))
        .route("/api/browser", get(crate::browsers::status).put(crate::browsers::set))
        .route(browser::ROUTE, get(crate::browsers::connect))
        .route(
            crate::browser_mcp::route(),
            post(crate::browser_mcp::post).get(crate::browser_mcp::other).delete(crate::browser_mcp::ended),
        )
        .route("/api/live", get(live))
        .route(
            "/api/{*rest}",
            any(|| async {
                ApiError::missing(said(
                    "There is no such address.",
                    "No existe esa dirección.",
                    "Não há esse endereço.",
                ))
            }),
        )
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
    app.layer(axum::middleware::from_fn_with_state(ctx.clone(), crate::error::localized))
        .layer(DefaultBodyLimit::max(MOST_BODY_BYTES))
        .layer(axum::middleware::from_fn(crate::access::logged))
        .with_state(ctx)
}
