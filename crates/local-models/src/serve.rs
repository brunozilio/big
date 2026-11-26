use std::sync::OnceLock;

use axum::Json;
use axum::body::{Body, Bytes};
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use serde_json::{Value, json};

use crate::LocalError;
use crate::download::transfers;
use crate::store::stored;

const PREFIX: &str = "/v1";

pub fn models() -> Value {
    let running = transfers();
    let listed = stored().into_iter().map(|model| {
        let transfer = running.iter().find(|transfer| transfer.id == model.id);
        json!({
            "id": model.id, "object": "model", "owned_by": "local", "bytes": model.bytes, "complete": model.complete,
            "received": transfer.map_or(model.bytes, |transfer| transfer.received),
            "total": transfer.and_then(|transfer| transfer.total),
            "running": transfer.is_some_and(|transfer| transfer.running),
            "error": transfer.and_then(|transfer| transfer.error.clone()),
        })
    });
    json!({ "object": "list", "data": listed.collect::<Vec<_>>() })
}

async fn listed() -> Json<Value> {
    Json(tokio::task::spawn_blocking(models).await.unwrap_or_else(|_| json!({ "object": "list", "data": [] })))
}

fn failed(error: &LocalError) -> Response {
    let status = match error {
        LocalError::Bad(_) | LocalError::Template(_) => StatusCode::BAD_REQUEST,
        LocalError::Missing(_) => StatusCode::NOT_FOUND,
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    };
    (status, Json(json!({ "error": { "message": error.to_string() } }))).into_response()
}

fn event(chunk: &Value) -> Bytes {
    Bytes::from(format!("data: {chunk}\n\n"))
}

async fn chat(Json(body): Json<Value>) -> Response {
    let streamed = body["stream"].as_bool().unwrap_or(false);
    let (sender, receiver) = tokio::sync::mpsc::channel::<Value>(256);
    let work = tokio::task::spawn_blocking(move || {
        let ended = crate::chat::complete(&body, &mut |chunk| sender.blocking_send(chunk).is_ok());
        if let Err(error) = &ended {
            tracing::warn!(%error, "a local model could not answer");
            let _ = sender.blocking_send(json!({ "error": { "message": error.to_string() } }));
        }
    });
    if !streamed {
        let mut receiver = receiver;
        let mut chunks = Vec::new();
        while let Some(chunk) = receiver.recv().await {
            chunks.push(chunk);
        }
        let _ = work.await;
        if let Some(error) = chunks.iter().find(|chunk| chunk["error"].is_object()) {
            let message = error["error"]["message"].as_str().unwrap_or_default().to_owned();
            return failed(&LocalError::Engine(message));
        }
        return Json(crate::chat::gathered(&chunks)).into_response();
    }
    let stream = futures_util::stream::unfold((receiver, false), |(mut receiver, done)| async move {
        if done {
            return None;
        }
        match receiver.recv().await {
            Some(chunk) => Some((Ok::<_, std::convert::Infallible>(event(&chunk)), (receiver, false))),
            None => Some((Ok(Bytes::from_static(b"data: [DONE]\n\n")), (receiver, true))),
        }
    });
    Response::builder()
        .header(header::CONTENT_TYPE, "text/event-stream")
        .header(header::CACHE_CONTROL, "no-cache")
        .body(Body::from_stream(stream))
        .unwrap_or_else(|error| failed(&LocalError::Engine(error.to_string())))
}

fn router() -> axum::Router {
    axum::Router::new()
        .route("/health", get(|| async { Json(json!({ "status": "ok" })) }))
        .route(&format!("{PREFIX}/models"), get(listed))
        .route(&format!("{PREFIX}/chat/completions"), post(chat))
}

fn started() -> Result<String, String> {
    let runtime = crate::runtime().map_err(|error| error.to_string())?;
    let bound = std::net::TcpListener::bind(("127.0.0.1", 0)).map_err(|error| error.to_string())?;
    bound.set_nonblocking(true).map_err(|error| error.to_string())?;
    let address = bound.local_addr().map_err(|error| error.to_string())?;
    let _inside = runtime.enter();
    let listener = tokio::net::TcpListener::from_std(bound).map_err(|error| error.to_string())?;
    runtime.spawn(async move {
        if let Err(error) = axum::serve(listener, router()).await {
            tracing::error!(%error, "the local model endpoint stopped");
        }
    });
    Ok(format!("http://{address}{PREFIX}"))
}

pub fn endpoint() -> Result<String, LocalError> {
    static ENDPOINT: OnceLock<Result<String, String>> = OnceLock::new();
    ENDPOINT.get_or_init(started).clone().map_err(LocalError::Engine)
}
