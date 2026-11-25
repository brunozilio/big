use std::sync::Arc;

use axum::extract::State;
use axum::response::Response;
use serde_json::{Value, json};

use crate::error::{ApiError, ApiResult, Spoken, json as reply, said};
use crate::state::{Ctx, Shared};

pub fn none() -> Spoken {
    said(
        "No model available: connect an AI provider.",
        "Ningún modelo disponible: conecta un proveedor de IA.",
        "Nenhum modelo disponível: conecte um provedor de IA.",
    )
}

pub fn listing(models: &[app::Model]) -> Value {
    if models.is_empty() {
        return json!({ "models": [], "default": null });
    }
    let listed = models.iter().map(|model| {
        let provider = model.provider.id();
        json!({
            "id": model.id, "name": model.label, "provider": provider, "logo": logo_of(&model.id, provider),
            "efforts": app::efforts(model), "fast": false,
        })
    });
    json!({ "models": listed.collect::<Vec<_>>(), "default": models.first().map(|model| &model.id) })
}

const MAKERS: [(&str, &str); 7] = [
    ("claude", "claude"),
    ("gpt", "openai"),
    ("codex", "openai"),
    ("o3", "openai"),
    ("o4", "openai"),
    ("grok", "grok"),
    ("gemini", "google"),
];

pub fn logo_of(model: &str, provider: &str) -> String {
    let id = model.to_ascii_lowercase();
    let words: Vec<&str> = id.split(|letter: char| !letter.is_ascii_alphanumeric()).collect();
    MAKERS
        .iter()
        .find(|(prefix, _)| words.iter().any(|word| word.starts_with(prefix)))
        .map_or_else(|| provider.trim_end_matches("-subscription").to_owned(), |(_, logo)| (*logo).to_owned())
}

fn unknown_effort() -> Spoken {
    said("Unknown effort.", "Esfuerzo desconocido.", "Esforço desconhecido.")
}

pub fn tuning(body: &Value) -> ApiResult<Option<app::Effort>> {
    if body["fast"] == Value::Bool(true) {
        return Err(ApiError::bad(said(
            "Fast mode is not available for this model.",
            "El modo rápido no está disponible para este modelo.",
            "O modo rápido não está disponível para este modelo.",
        )));
    }
    match &body["effort"] {
        Value::Null => Ok(None),
        Value::String(name) => app::Effort::named(name).map(Some).ok_or_else(|| ApiError::bad(unknown_effort())),
        _ => Err(ApiError::bad(unknown_effort())),
    }
}

pub fn known(shared: &Shared) -> ApiResult<Vec<app::Model>> {
    let models = shared.backend.client.models().map_err(ApiError::failed)?;
    if let Some(engine) = &shared.backend.engine {
        engine.remember(models.clone());
    }
    Ok(models)
}

pub async fn flows() -> ApiResult<Response> {
    let found = tokio::task::spawn_blocking(orchestrator::flows::names).await.map_err(ApiError::failed)?;
    Ok(reply(serde_json::json!(found)))
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
    fn without_models_there_is_no_default() {
        assert_eq!(listing(&[]), json!({ "models": [], "default": null }));
    }

    #[test]
    fn a_model_lists_the_efforts_of_its_family() {
        let listed = listing(&[app::Model::new("anthropic:claude-opus-5-5", "Claude Opus 5.5")]);
        assert_eq!(listed["default"], "anthropic:claude-opus-5-5");
        assert_eq!(listed["models"][0]["efforts"], json!(["low", "medium", "high", "xhigh", "max"]));
        assert_eq!(listed["models"][0]["fast"], false);
    }

    #[test]
    fn a_kiro_model_lists_the_efforts_its_catalog_declared() {
        let mut opus = app::Model::new("kiro-subscription:claude-opus-4.6", "Claude Opus 4.6");
        opus.capabilities = Some(domain::model::ModelCapabilities {
            reasoning: true,
            adaptive_thinking: true,
            reasoning_levels: ["low", "medium", "high", "max"].map(String::from).to_vec(),
            ..Default::default()
        });
        let listed = listing(&[opus, app::Model::new("kiro-subscription:claude-sonnet-4.5", "Claude Sonnet 4.5")]);
        assert_eq!(listed["models"][0]["efforts"], json!(["low", "medium", "high", "max"]));
        assert_eq!(listed["models"][1]["efforts"], json!([]));
    }

    #[test]
    fn a_provider_native_auto_is_listed_like_any_other_model() {
        let native = app::Model::new("kiro-subscription:auto", "Auto")
            .served_by(domain::ModelProvider::new("kiro-subscription"));
        let listed = listing(&[native]);
        assert_eq!(listed["models"][0]["id"], "kiro-subscription:auto");
        assert_eq!(listed["models"][0]["provider"], "kiro-subscription");
        assert_eq!(listed["models"][0]["logo"], "kiro");
    }

    #[test]
    fn the_effort_is_read_and_fast_is_refused() {
        assert_eq!(tuning(&json!({ "effort": "low" })).ok(), Some(Some(app::Effort::Low)));
        assert_eq!(tuning(&json!({})).ok(), Some(None));
        assert!(tuning(&json!({ "effort": 3 })).is_err());
        assert!(tuning(&json!({ "fast": true })).is_err());
    }
}

#[cfg(test)]
mod logo_tests {
    use super::logo_of;

    #[test]
    fn the_logo_is_the_maker_of_the_model_whatever_account_serves_it() {
        assert_eq!(logo_of("claude-sonnet-4-5", "copilot-subscription"), "claude");
        assert_eq!(logo_of("gpt-5", "copilot-subscription"), "openai");
        assert_eq!(logo_of("gpt-5-codex", "openai-subscription"), "openai");
        assert_eq!(logo_of("grok-4", "grok-subscription"), "grok");
        assert_eq!(logo_of("gemini-3-pro", "antigravity-subscription"), "google");
        assert_eq!(logo_of("us.anthropic.claude-opus-4", "kiro-subscription"), "claude");
        assert_eq!(logo_of("auto-kiro", "kiro-subscription"), "kiro");
    }
}
