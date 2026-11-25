use axum::Json;
use axum::extract::State;
use axum::http::header;
use axum::response::{IntoResponse, Response};
use domain::Language;
use serde_json::{Value, json};
use store::Store;

use crate::error::{ApiError, ApiResult, json as reply, said};
use crate::state::Ctx;

pub const LOGO_ROUTE: &str = "/api/branding/logo";
pub const FAVICON_ROUTE: &str = "/api/branding/favicon";
const THEMES: [&str; 3] = ["system", "light", "dark"];

pub fn current(store: &Store) -> ApiResult<Value> {
    let kept = store.branding()?;
    let theme = kept.get("theme").and_then(Value::as_str).filter(|theme| THEMES.contains(theme)).unwrap_or("system");
    let styles = app::output_styles();
    let style =
        kept.get("output_style").and_then(Value::as_str).filter(|key| styles.iter().any(|style| style.key == *key));
    Ok(json!({
        "name": brand::NAME, "logo": LOGO_ROUTE, "favicon": FAVICON_ROUTE, "editor": crate::editor::editor_name(), "theme": theme, "language": store.language()?.code(),
        "output_style": style, "output_styles": styles,
    }))
}

pub fn apply(store: &Store, patch: &Value) -> ApiResult<Value> {
    if let Some(theme) = patch.get("theme") {
        let theme = theme.as_str().filter(|theme| THEMES.contains(theme));
        let theme = theme.ok_or_else(|| {
            ApiError::bad(said(
                "The theme is system, light or dark.",
                "El tema es system, light o dark.",
                "O tema é system, light ou dark.",
            ))
        })?;
        store.set_branding("theme", &json!(theme))?;
    }
    if let Some(language) = patch.get("language") {
        let language = language.as_str().and_then(Language::named);
        let language = language.ok_or_else(|| {
            ApiError::bad(said(
                "The language is en, es or pt-BR.",
                "El idioma es en, es o pt-BR.",
                "O idioma é en, es ou pt-BR.",
            ))
        })?;
        store.set_branding("language", &json!(language.code()))?;
    }
    if let Some(style) = patch.get("output_style") {
        let known = |key: &str| app::output_styles().iter().any(|style| style.key == key);
        match style {
            Value::Null => store.set_branding("output_style", &Value::Null)?,
            Value::String(key) if known(key) => store.set_branding("output_style", &json!(key))?,
            _ => {
                return Err(ApiError::bad(said(
                    "That response style does not exist.",
                    "Ese estilo de respuesta no existe.",
                    "Esse estilo de resposta não existe.",
                )));
            }
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

pub async fn logo() -> Response {
    png(brand::LOGO)
}

pub async fn favicon() -> Response {
    png(brand::FAVICON)
}

fn png(bytes: &'static [u8]) -> Response {
    ([(header::CONTENT_TYPE, "image/png"), (header::CACHE_CONTROL, "no-cache")], bytes).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_name_and_logo_are_the_brand_s_and_the_theme_is_kept() {
        let store = Store::in_memory(0).expect("store");
        let first = current(&store).expect("current");
        assert_eq!(first["theme"], "system");
        assert_eq!(first["name"], brand::NAME);
        assert_eq!(first["output_style"], Value::Null);
        let saved = apply(&store, &json!({ "theme": "dark", "logo": "data:x" })).expect("saved");
        assert_eq!(saved["theme"], "dark");
        assert_eq!(saved["logo"], LOGO_ROUTE);
        assert_eq!(saved["favicon"], FAVICON_ROUTE);
        assert!(apply(&store, &json!({ "theme": "pink" })).is_err());
    }

    #[test]
    fn the_language_starts_in_portuguese_and_only_a_known_one_is_kept() {
        let store = Store::in_memory(0).expect("store");
        assert_eq!(current(&store).expect("current")["language"], "pt-BR");
        assert_eq!(apply(&store, &json!({ "language": "en" })).expect("saved")["language"], "en");
        assert_eq!(store.language().expect("language"), Language::English);
        assert!(apply(&store, &json!({ "language": "fr" })).is_err());
        assert!(apply(&store, &json!({ "language": 3 })).is_err());
        assert_eq!(current(&store).expect("current")["language"], "en");
    }

    #[test]
    fn the_output_style_is_one_of_the_listed_or_none() {
        let store = Store::in_memory(0).expect("store");
        let listed = current(&store).expect("current")["output_styles"].clone();
        assert!(listed.as_array().is_some_and(|styles| styles.iter().any(|style| style["key"] == "concise")));
        assert_eq!(apply(&store, &json!({ "output_style": "concise" })).expect("saved")["output_style"], "concise");
        assert_eq!(store.output_style().as_deref(), Some("concise"));
        assert!(apply(&store, &json!({ "output_style": "nope" })).is_err());
        assert_eq!(apply(&store, &json!({ "output_style": null })).expect("cleared")["output_style"], Value::Null);
        assert_eq!(store.output_style(), None);
    }
}
