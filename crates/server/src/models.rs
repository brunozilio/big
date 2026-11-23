use std::sync::Arc;

use axum::extract::State;
use axum::response::Response;
use serde_json::{Value, json};

use crate::error::{ApiError, ApiResult, json as reply};
use crate::state::{Ctx, Shared};

pub const AUTO: &str = "auto";

pub const NONE: &str = "Nenhum modelo disponível: conecte um provedor de IA.";

pub fn listing(models: &[app::Model]) -> Value {
    if models.is_empty() {
        return json!({ "models": [], "default": null });
    }
    let auto = json!({
        "id": AUTO, "name": "Auto", "provider": AUTO, "logo": "",
        "description": "Escolhe o modelo de cada etapa",
        "efforts": app::Effort::ALL.map(app::Effort::cli_value), "fast": false,
    });
    let listed = models.iter().map(|model| {
        let provider = model.provider.id();
        json!({
            "id": model.id, "name": model.label, "provider": provider, "logo": provider,
            "efforts": app::efforts(model), "fast": false,
        })
    });
    json!({ "models": std::iter::once(auto).chain(listed).collect::<Vec<_>>(), "default": AUTO })
}

pub fn tuning(body: &Value) -> ApiResult<Option<app::Effort>> {
    if body["fast"] == Value::Bool(true) {
        return Err(ApiError::bad("O modo rápido não está disponível para este modelo."));
    }
    match &body["effort"] {
        Value::Null => Ok(None),
        Value::String(name) => app::Effort::named(name).map(Some).ok_or_else(|| ApiError::bad("Esforço desconhecido.")),
        _ => Err(ApiError::bad("Esforço desconhecido.")),
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn without_models_there_is_no_auto_and_no_default() {
        assert_eq!(listing(&[]), json!({ "models": [], "default": null }));
    }

    #[test]
    fn a_model_lists_the_efforts_of_its_family() {
        let listed = listing(&[app::Model::new("anthropic:claude-opus-5-5", "Claude Opus 5.5")]);
        assert_eq!(listed["models"][1]["efforts"], json!(["low", "medium", "high", "xhigh", "max"]));
        assert_eq!(listed["models"][1]["fast"], false);
    }

    #[test]
    fn the_effort_is_read_and_fast_is_refused() {
        assert_eq!(tuning(&json!({ "effort": "low" })).ok(), Some(Some(app::Effort::Low)));
        assert_eq!(tuning(&json!({})).ok(), Some(None));
        assert!(tuning(&json!({ "effort": 3 })).is_err());
        assert!(tuning(&json!({ "fast": true })).is_err());
    }
}
