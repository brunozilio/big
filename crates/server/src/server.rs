use std::collections::HashMap;
use std::net::{Ipv4Addr, SocketAddr, TcpListener};
use std::sync::{Arc, Mutex};

use axum::Router;
use tokio::sync::mpsc;

use crate::backend::Backend;
use crate::chats::Chats;
use crate::live::Live;
use crate::sign_in::Flows;
use crate::stamps::Stamps;
use crate::state::{Ctx, Shared};

pub fn bind(port: u16) -> std::io::Result<TcpListener> {
    let listener = match TcpListener::bind((Ipv4Addr::LOCALHOST, port)) {
        Ok(listener) => listener,
        Err(error) if port != 0 => {
            tracing::warn!(%error, port, "the configured port is taken; another free one is used");
            TcpListener::bind((Ipv4Addr::LOCALHOST, 0))?
        }
        Err(error) => return Err(error),
    };
    listener.set_nonblocking(true)?;
    Ok(listener)
}

pub fn app(backend: Backend, bundle: &'static [page::Asset], port: u16) -> (Router, Live) {
    let (nudge, nudges) = mpsc::unbounded_channel();
    let ctx: Ctx = Arc::new(Shared {
        backend,
        live: Live::default(),
        stamps: Stamps::default(),
        chats: Chats::default(),
        watched: crate::watched::Watched::default(),
        flows: Flows::default(),
        followed: Mutex::new(HashMap::new()),
        answered: Mutex::new(HashMap::new()),
        nudge,
        port,
        callbacks: Mutex::new(std::collections::HashSet::new()),
        bundle,
    });
    let weak = Arc::downgrade(&ctx);
    ctx.backend.orchestrator.watch(Arc::new(move |session, event| {
        if let Some(shared) = weak.upgrade() {
            crate::sessions::watched(&shared, session, event);
        }
    }));
    tokio::spawn(crate::pump::orchestrator_loop(Arc::clone(&ctx)));
    tokio::spawn(crate::pump::health_loop(Arc::clone(&ctx)));
    tokio::spawn(crate::sessions::follow_loop(Arc::clone(&ctx), nudges));
    (crate::routes::router(Arc::clone(&ctx)), ctx.live.clone())
}

pub async fn serve(
    listener: TcpListener,
    backend: Backend,
    bundle: &'static [page::Asset],
    stop: impl std::future::Future<Output = ()> + Send + 'static,
) -> std::io::Result<()> {
    let address: SocketAddr = listener.local_addr()?;
    let listener = tokio::net::TcpListener::from_std(listener)?;
    let (router, _ctx) = app(backend, bundle, address.port());
    let (stopped, on_stop) = tokio::sync::oneshot::channel::<()>();
    let serving = axum::serve(listener, router).with_graceful_shutdown(async move {
        stop.await;
        let _ = stopped.send(());
    });
    let deadline = async move {
        match on_stop.await {
            Ok(()) => tokio::time::sleep(SHUTDOWN_GRACE).await,
            Err(_) => std::future::pending::<()>().await,
        }
    };
    tokio::select! {
        served = std::future::IntoFuture::into_future(serving) => served,
        () = deadline => Ok(()),
    }
}

const SHUTDOWN_GRACE: std::time::Duration = std::time::Duration::from_secs(3);
