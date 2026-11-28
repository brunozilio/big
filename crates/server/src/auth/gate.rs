use axum::extract::{Request, State};
use axum::http::{Method, StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

use super::{session, settings};
use crate::error::{ApiError, said};
use crate::state::Ctx;

fn open(method: &Method, path: &str) -> bool {
    let reads = method == Method::GET || method == Method::HEAD;
    path.starts_with("/api/auth/")
        || path.starts_with(orchestrator::CLOUD_CREDENTIALS_PATH)
        || path.starts_with(crate::workspaces::CREDENTIALS_PATH)
        || path == crate::mcp::route()
        || path == browser::ROUTE
        || crate::browser_devices::open_path(path)
        || (reads
            && (path == "/api/branding"
                || path == crate::branding::LOGO_ROUTE
                || path == crate::branding::FAVICON_ROUTE))
}

fn guarded(path: &str) -> bool {
    path.starts_with("/api/") || path == "/oauth/callback" || path.starts_with("/oauth/loopback/")
}

fn same_origin(request: &Request, origin: &str) -> bool {
    let method = request.method();
    if method == Method::GET || method == Method::HEAD || method == Method::OPTIONS {
        return true;
    }
    match request.headers().get(header::ORIGIN).and_then(|value| value.to_str().ok()) {
        Some(sent) => sent.trim_end_matches('/') == origin,
        None => true,
    }
}

pub fn signed_out_error() -> ApiError {
    ApiError::new(
        StatusCode::UNAUTHORIZED,
        "signed_out",
        said("Sign in to continue.", "Inicia sesión para continuar.", "Entre para continuar."),
    )
}

pub async fn gate(State(ctx): State<Ctx>, mut request: Request, next: Next) -> Response {
    let path = request.uri().path().to_owned();
    if !guarded(&path) || open(request.method(), &path) {
        return next.run(request).await;
    }
    if settings::mode() == settings::Mode::Shared && !same_origin(&request, &settings::public_origin(ctx.port)) {
        return ApiError::new(
            StatusCode::FORBIDDEN,
            "cross_origin",
            said(
                "This request came from another site.",
                "Esta solicitud vino de otro sitio.",
                "Este pedido veio de outro site.",
            ),
        )
        .into_response();
    }
    match session::person(&ctx, request.headers()) {
        Ok(Some(person)) => {
            if let Err(refused) = crate::scope::check(&ctx, &person, request.method(), &path) {
                return refused.into_response();
            }
            request.extensions_mut().insert(person);
            next.run(request).await
        }
        Ok(None) => signed_out_error().into_response(),
        Err(error) => error.into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_page_sign_in_and_branding_reads_stay_open() {
        assert!(!guarded("/"));
        assert!(!guarded("/items/1"));
        assert!(guarded("/api/items"));
        assert!(guarded("/oauth/callback"));
        assert!(guarded("/oauth/loopback/1455/auth/callback"));
        assert!(open(&Method::POST, "/api/auth/email"));
        assert!(open(&Method::GET, "/api/branding"));
        assert!(!open(&Method::PUT, "/api/branding"));
        assert!(!open(&Method::GET, "/api/items"));
    }
}
