use std::time::Instant;

use axum::extract::Request;
use axum::http::Method;
use axum::middleware::Next;
use axum::response::Response;

pub async fn logged(request: Request, next: Next) -> Response {
    let started = Instant::now();
    let method = request.method().clone();
    let path = request.uri().path().to_owned();
    let response = next.run(request).await;
    let status = response.status().as_u16();
    told(&method, &path, status, started.elapsed().as_millis() as u64);
    response
}

fn told(method: &Method, path: &str, status: u16, duration_ms: u64) {
    let level = level(method, status);
    let method = method.as_str();
    match level {
        Level::Failed => {
            tracing::event!(target: "audit", tracing::Level::ERROR, http.request.method = method, url.path = path, http.response.status_code = status, duration_ms, outcome = "failure", "http.server.request")
        }
        Level::Refused => {
            tracing::event!(target: "audit", tracing::Level::WARN, http.request.method = method, url.path = path, http.response.status_code = status, duration_ms, outcome = "failure", "http.server.request")
        }
        Level::Changed => {
            tracing::event!(target: "audit", tracing::Level::INFO, http.request.method = method, url.path = path, http.response.status_code = status, duration_ms, outcome = "success", "http.server.request")
        }
        Level::Read => {
            tracing::event!(target: "audit", tracing::Level::DEBUG, http.request.method = method, url.path = path, http.response.status_code = status, duration_ms, outcome = "success", "http.server.request")
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
enum Level {
    Failed,
    Refused,
    Changed,
    Read,
}

fn level(method: &Method, status: u16) -> Level {
    match status {
        500.. => Level::Failed,
        400.. => Level::Refused,
        _ if method == Method::GET || method == Method::HEAD || method == Method::OPTIONS => Level::Read,
        _ => Level::Changed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_are_quieter_than_changes_and_failures_are_loudest() {
        assert_eq!(level(&Method::GET, 200), Level::Read);
        assert_eq!(level(&Method::POST, 201), Level::Changed);
        assert_eq!(level(&Method::GET, 404), Level::Refused);
        assert_eq!(level(&Method::DELETE, 503), Level::Failed);
    }
}
