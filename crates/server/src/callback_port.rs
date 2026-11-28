use std::collections::HashMap;
use std::net::Ipv4Addr;
use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::body::Bytes;
use axum::extract::{Query, State};
use axum::http::{HeaderMap, HeaderValue, Method, StatusCode, Uri, header};
use axum::response::{IntoResponse, Response};
use serde_json::{Value, json};
use tokio::net::TcpListener;
use tokio::sync::Notify;

use crate::error::{ApiError, ApiResult, said};
use crate::state::{Ctx, held};

const WAITS: Duration = Duration::from_secs(1800);

type Door = (Ctx, Arc<Notify>);

async fn returned(
    State((ctx, done)): State<Door>,
    method: Method,
    headers: HeaderMap,
    uri: Uri,
    Query(query): Query<HashMap<String, String>>,
    body: Bytes,
) -> Response {
    let origin = headers.get(header::ORIGIN).and_then(|origin| origin.to_str().ok()).unwrap_or_default();
    let listening = (!origin.is_empty()).then(|| ctx.flows.listening(origin)).flatten();
    let (answer, finished) = match listening {
        Some(flow) => asked(&ctx, &flow, &method, &headers, &uri, &query, &body).await,
        None if !origin.is_empty() && method == Method::OPTIONS => (StatusCode::FORBIDDEN.into_response(), false),
        None => {
            let base = format!("http://127.0.0.1:{}", ctx.port);
            crate::connections::hopped(&ctx, &query, &base).await
        }
    };
    if finished {
        done.notify_one();
    }
    answer
}

async fn asked(
    ctx: &Ctx,
    flow: &str,
    method: &Method,
    headers: &HeaderMap,
    uri: &Uri,
    query: &HashMap<String, String>,
    body: &Bytes,
) -> (Response, bool) {
    let origin = headers.get(header::ORIGIN).cloned().unwrap_or(HeaderValue::from_static(""));
    if method == Method::OPTIONS {
        return (allowed(StatusCode::NO_CONTENT.into_response(), origin, headers), false);
    }
    let text = String::from_utf8_lossy(body);
    let from = origin.to_str().unwrap_or_default();
    let request = crate::sign_in::request_of(method.as_str(), uri.path(), &json!(query), from, &text);
    let (status, body, finished) = crate::sign_in::answered(ctx, flow, &request).await;
    let status = StatusCode::from_u16(status).unwrap_or(StatusCode::OK);
    let response = match body {
        Value::String(text) => (status, text).into_response(),
        Value::Null => status.into_response(),
        other => (status, axum::Json(other)).into_response(),
    };
    (allowed(response, origin, headers), finished)
}

fn allowed(mut response: Response, origin: HeaderValue, asked: &HeaderMap) -> Response {
    let headers = response.headers_mut();
    headers.insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, origin);
    headers.insert(header::VARY, HeaderValue::from_static("Origin"));
    headers.insert(header::ACCESS_CONTROL_ALLOW_METHODS, HeaderValue::from_static("GET, POST, OPTIONS"));
    let wanted = asked.get(header::ACCESS_CONTROL_REQUEST_HEADERS).cloned();
    headers.insert(header::ACCESS_CONTROL_ALLOW_HEADERS, wanted.unwrap_or(HeaderValue::from_static("content-type")));
    headers.insert("access-control-allow-private-network", HeaderValue::from_static("true"));
    response
}

fn taken(from: u16, to: u16, error: &std::io::Error) -> ApiError {
    let ports = if from == to { format!("{from}") } else { format!("{from}–{to}") };
    ApiError::bad(said(
        format!(
            "Port {ports}, where the sign-in comes back, is taken: {error}. Close whatever uses that port and try again."
        ),
        format!(
            "El puerto {ports}, por donde vuelve el inicio de sesión, está ocupado: {error}. Cierra lo que usa ese puerto e inténtalo de nuevo."
        ),
        format!(
            "A porta {ports}, por onde a entrada volta, está ocupada: {error}. Feche o que usa essa porta e tente de novo."
        ),
    ))
}

async fn bound(from: u16, to: u16) -> ApiResult<(TcpListener, u16)> {
    let mut last = std::io::Error::other("no port to try");
    for port in from..=to {
        match TcpListener::bind((Ipv4Addr::LOCALHOST, port)).await {
            Ok(listener) => return Ok((listener, port)),
            Err(error) => last = error,
        }
    }
    Err(taken(from, to, &last))
}

pub async fn open(ctx: &Ctx, from: u16, to: u16) -> ApiResult<u16> {
    if let Some(port) = (from..=to).find(|port| held(&ctx.callbacks).contains(port)) {
        return Ok(port);
    }
    let (listener, port) = bound(from, to).await?;
    held(&ctx.callbacks).insert(port);
    let done = Arc::new(Notify::new());
    let router = Router::new().fallback(returned).with_state((Arc::clone(ctx), Arc::clone(&done)));
    let shared = Arc::clone(ctx);
    tokio::spawn(async move {
        let stop = async move {
            let _ = tokio::time::timeout(WAITS, done.notified()).await;
        };
        if let Err(error) = axum::serve(listener, router).with_graceful_shutdown(stop).await {
            tracing::warn!(%error, port, "the sign-in callback port stopped");
        }
        held(&shared.callbacks).remove(&port);
    });
    Ok(port)
}
