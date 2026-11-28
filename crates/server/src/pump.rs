use std::sync::Arc;
use std::time::Duration;

use domain::workflow::ItemId;
use orchestrator::OrchestratorEvent;
use tokio::sync::broadcast::error::RecvError;

use crate::state::{Ctx, Shared};

const HEALTH_EVERY: Duration = Duration::from_secs(5);

fn item_changed(shared: &Shared, id: &ItemId) {
    match crate::items::changed(shared, id) {
        Ok(Some(item)) => shared.live.item(item),
        Ok(None) => {
            if shared.backend.orchestrator.item(id).ok().flatten().is_none() && shared.stamps.forget(id.as_str()) {
                shared.live.removed(id.as_str());
            }
        }
        Err(error) => tracing::warn!(item = %id, message = %error.message, "an item could not be shown"),
    }
}

fn handle(shared: &Shared, event: OrchestratorEvent) {
    match event {
        OrchestratorEvent::ItemChanged { item, .. } => item_changed(shared, &item),
        OrchestratorEvent::ItemEnded { item } => {
            crate::inbox::forget_claims(shared, &item);
            item_changed(shared, &item);
        }
        OrchestratorEvent::Session { item, session, .. } => {
            shared.follow(&session);
            item_changed(shared, &item);
        }
        OrchestratorEvent::Asked { item, session, .. } => {
            if let Some(session) = session {
                shared.follow(&session);
            }
            item_changed(shared, &item);
        }
        OrchestratorEvent::AskClosed { item, .. } => item_changed(shared, &item),
    }
}

fn resync(shared: &Shared) {
    let Ok(items) = crate::items::all(shared) else { return };
    for item in items {
        shared.live.item(item);
    }
}

pub async fn orchestrator_loop(ctx: Ctx) {
    let mut events = ctx.backend.orchestrator.subscribe();
    loop {
        let event = match events.recv().await {
            Ok(event) => Some(event),
            Err(RecvError::Lagged(_)) => None,
            Err(RecvError::Closed) => return,
        };
        let shared = Arc::clone(&ctx);
        let _ = tokio::task::spawn_blocking(move || match event {
            Some(event) => handle(&shared, event),
            None => resync(&shared),
        })
        .await;
    }
}

fn standing(shared: &Shared) -> String {
    let health = shared.backend.orchestrator.health().map(|health| health.all());
    let lapsed = shared.backend.connectors.keeper().lapsed();
    let ai: Vec<(String, bool)> = crate::ai::subscriptions(shared)
        .iter()
        .flat_map(|summary| {
            summary.accounts.iter().chain(&summary.account).map(|account| (account.id.clone(), account.lapsed))
        })
        .collect();
    format!("{health:?}{lapsed:?}{ai:?}")
}

pub async fn health_loop(ctx: Ctx) {
    let mut seen = String::new();
    let mut tick = tokio::time::interval(HEALTH_EVERY);
    loop {
        tick.tick().await;
        let shared = Arc::clone(&ctx);
        let Ok(now) = tokio::task::spawn_blocking(move || standing(&shared)).await else { continue };
        if now != seen {
            seen = now;
            let shared = Arc::clone(&ctx);
            let _ = tokio::task::spawn_blocking(move || crate::connections::announce(&shared)).await;
        }
    }
}
