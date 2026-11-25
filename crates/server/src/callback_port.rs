use std::collections::HashMap;
use std::net::Ipv4Addr;
use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::extract::{Query, State};
use axum::response::Response;
use axum::routing::get;
use tokio::sync::Notify;

use crate::error::{ApiError, ApiResult, said};
use crate::state::{Ctx, held};

const WAITS: Duration = Duration::from_secs(1800);

type Door = (Ctx, Arc<Notify>);

async fn returned(State((ctx, done)): State<Door>, Query(query): Query<HashMap<String, String>>) -> Response {
    let base = format!("http://127.0.0.1:{}", ctx.port);
    let answer = crate::connections::answered(&ctx, &query, &base).await;
    done.notify_one();
    answer
}

pub async fn open(ctx: &Ctx, port: u16) -> ApiResult<()> {
    if held(&ctx.callbacks).contains(&port) {
        return Ok(());
    }
    let listener = tokio::net::TcpListener::bind((Ipv4Addr::LOCALHOST, port)).await.map_err(|error| {
        ApiError::bad(said(
            format!("Port {port}, where the sign-in comes back, is taken: {error}. Close whatever uses that port and try again."),
            format!("El puerto {port}, por donde vuelve el inicio de sesión, está ocupado: {error}. Cierra lo que usa ese puerto e inténtalo de nuevo."),
            format!("A porta {port}, por onde a entrada volta, está ocupada: {error}. Feche o que usa essa porta e tente de novo."),
        ))
    })?;
    held(&ctx.callbacks).insert(port);
    let done = Arc::new(Notify::new());
    let router = Router::new().route("/oauth/callback", get(returned)).with_state((Arc::clone(ctx), Arc::clone(&done)));
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
    Ok(())
}
