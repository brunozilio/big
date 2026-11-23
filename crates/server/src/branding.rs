use axum::Json;
use axum::extract::State;
use axum::response::Response;
use serde_json::{Value, json};
use store::Store;

use crate::error::{ApiError, ApiResult, json as reply};
use crate::state::Ctx;

const THEMES: [&str; 3] = ["system", "light", "dark"];
const MOST_LOGO_BYTES: usize = 2 * 1024 * 1024;

pub fn current(store: &Store) -> ApiResult<Value> {
    let kept = store.branding()?;
    let logo = kept.get("logo").and_then(Value::as_str).filter(|logo| !logo.is_empty());
    let theme = kept.get("theme").and_then(Value::as_str).filter(|theme| THEMES.contains(theme)).unwrap_or("system");
    Ok(json!({ "name": brand::NAME, "logo": logo, "theme": theme }))
}

pub fn apply(store: &Store, patch: &Value) -> ApiResult<Value> {
    if let Some(theme) = patch.get("theme") {
        let theme = theme.as_str().filter(|theme| THEMES.contains(theme));
        let theme = theme.ok_or_else(|| ApiError::bad("O tema é system, light ou dark."))?;
        store.set_branding("theme", &json!(theme))?;
    }
    if let Some(logo) = patch.get("logo") {
        match logo {
            Value::Null => store.set_branding("logo", &json!(""))?,
            Value::String(text) if text.len() <= MOST_LOGO_BYTES => store.set_branding("logo", logo)?,
            Value::String(_) => return Err(ApiError::bad("O logo passa de 2 MB.")),
            _ => return Err(ApiError::bad("O logo é um texto (URL ou data URL) ou null.")),
        }
    }
    current(store)
}

pub async fn get(State(ctx): State<Ctx>) -> ApiResult<Response> {
    Ok(reply(current(&ctx.backend.store)?))
}

pub async fn put(State(ctx): State<Ctx>, Json(patch): Json<Value>) -> ApiResult<Response> {
    let branding = apply(&ctx.backend.store, &patch)?;
    ctx.live.branding(branding.clone());
    Ok(reply(branding))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_name_is_the_brand_s_and_the_rest_is_kept() {
        let store = Store::in_memory(0).expect("store");
        assert_eq!(current(&store).ok(), Some(json!({ "name": brand::NAME, "logo": null, "theme": "system" })));
        let saved = apply(&store, &json!({ "theme": "dark", "logo": "data:x" })).expect("saved");
        assert_eq!(saved["theme"], "dark");
        assert_eq!(apply(&store, &json!({ "logo": null })).expect("cleared")["logo"], Value::Null);
        assert!(apply(&store, &json!({ "theme": "pink" })).is_err());
    }
}
