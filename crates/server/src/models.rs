use std::sync::Arc;

use axum::extract::State;
use axum::response::Response;
use serde_json::{Value, json};

use crate::error::{ApiError, ApiResult, json as reply};
use crate::state::{Ctx, Shared};

pub const AUTO: &str = "auto";

pub fn listing(models: &[app::Model]) -> Value {
    let auto = json!({
        "id": AUTO, "name": "Auto", "provider": AUTO, "logo": "",
        "description": "Escolhe o modelo de cada etapa",
    });
    let listed = models.iter().map(|model| {
        let provider = model.provider.id();
        json!({ "id": model.id, "name": model.label, "provider": provider, "logo": provider })
    });
    json!({ "models": std::iter::once(auto).chain(listed).collect::<Vec<_>>(), "default": AUTO })
}

pub fn known(shared: &Shared) -> ApiResult<Vec<app::Model>> {
    let models = shared.backend.client.models().map_err(ApiError::failed)?;
    if let Some(engine) = &shared.backend.engine {
        engine.remember(models.clone());
    }
    Ok(models)
}

pub async fn list(State(ctx): State<Ctx>) -> ApiResult<Response> {
    let shared = Arc::clone(&ctx);
    let models = tokio::task::spawn_blocking(move || known(&shared)).await.map_err(ApiError::failed)??;
    Ok(reply(listing(&models)))
}
