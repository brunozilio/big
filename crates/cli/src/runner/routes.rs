use std::collections::HashMap;
use std::convert::Infallible;
use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::body::Body;
use axum::extract::{Query, Request, State};
use axum::http::header::{COOKIE, HOST, LOCATION, SET_COOKIE};
use axum::http::{HeaderMap, StatusCode};
use axum::middleware::{self, Next};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use futures_util::stream::{self, StreamExt};

use super::{KEY_COOKIE, Runner};

type Shared = (Arc<Runner>, u16);

pub fn router(runner: Arc<Runner>, port: u16) -> Router {
    let shared: Shared = (runner, port);
    Router::new()
        .route("/run", get(enter))
        .route("/api/runner/events", get(events))
        .route("/api/runner/answer", post(answer))
        .fallback(other)
        .layer(middleware::from_fn_with_state(shared.clone(), guard))
        .with_state(shared)
}

fn local_host(headers: &HeaderMap, port: u16) -> bool {
    let host = headers.get(HOST).and_then(|value| value.to_str().ok()).unwrap_or_default();
    host == format!("127.0.0.1:{port}") || host == format!("localhost:{port}")
}

fn holds_key(headers: &HeaderMap, key: &str) -> bool {
    let wanted = format!("{KEY_COOKIE}={key}");
    headers
        .get_all(COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(';'))
        .any(|pair| pair.trim() == wanted)
}

async fn guard(State((runner, port)): State<Shared>, request: Request, next: Next) -> Response {
    if !local_host(request.headers(), port) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let entering = request.uri().path() == "/run" && request.uri().query().is_some_and(|query| query.contains("key="));
    if !entering && !holds_key(request.headers(), runner.key()) {
        return StatusCode::FORBIDDEN.into_response();
    }
    next.run(request).await
}

async fn enter(State((runner, _)): State<Shared>, Query(query): Query<HashMap<String, String>>) -> Response {
    if query.get("key").map(String::as_str) != Some(runner.key()) {
        if query.contains_key("key") {
            return StatusCode::FORBIDDEN.into_response();
        }
        return server::serve_page(&"/run".parse().unwrap_or_default());
    }
    let cookie = format!("{KEY_COOKIE}={}; Path=/; HttpOnly; SameSite=Strict", runner.key());
    (StatusCode::SEE_OTHER, [(LOCATION, "/run".to_owned()), (SET_COOKIE, cookie)]).into_response()
}

async fn events(State((runner, _)): State<Shared>) -> Response {
    let (history, live) = runner.so_far();
    let later = stream::unfold(live, |mut live| async move {
        loop {
            match live.recv().await {
                Ok(line) => return Some((line, live)),
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {}
                Err(tokio::sync::broadcast::error::RecvError::Closed) => return None,
            }
        }
    });
    let lines = stream::iter(history).chain(later).map(|line| Ok::<Event, Infallible>(Event::default().data(line)));
    Sse::new(lines).keep_alive(KeepAlive::new().interval(Duration::from_secs(15))).into_response()
}

async fn answer(State((runner, _)): State<Shared>, body: String) -> Response {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(&body) else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    if runner.answer(value.to_string()).await { StatusCode::NO_CONTENT } else { StatusCode::GONE }.into_response()
}

async fn other(State((runner, _)): State<Shared>, request: Request<Body>) -> Response {
    let path = request.uri().path();
    let remote_path = path.starts_with("/api/") || path.starts_with("/oauth/");
    match runner.remote() {
        Some(remote) if remote_path => super::proxy::forward(&remote, request).await,
        _ if remote_path => StatusCode::SERVICE_UNAVAILABLE.into_response(),
        _ => server::serve_page(request.uri()),
    }
}
