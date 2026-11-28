use axum::Json;
use axum::extract::{Request, State};
use axum::http::StatusCode;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use domain::Language;
use serde::Serialize;
use serde_json::json;

use crate::state::Ctx;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Spoken {
    english: String,
    spanish: String,
    portuguese: String,
}

impl Spoken {
    pub fn in_language(&self, language: Language) -> String {
        language.pick(&self.english, &self.spanish, &self.portuguese).clone()
    }
}

pub fn said(english: impl Into<String>, spanish: impl Into<String>, portuguese: impl Into<String>) -> Spoken {
    Spoken { english: english.into(), spanish: spanish.into(), portuguese: portuguese.into() }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Told {
    message: String,
    spoken: Option<Spoken>,
}

impl From<&str> for Told {
    fn from(message: &str) -> Told {
        Told { message: message.to_owned(), spoken: None }
    }
}

impl From<String> for Told {
    fn from(message: String) -> Told {
        Told { message, spoken: None }
    }
}

impl From<Spoken> for Told {
    fn from(spoken: Spoken) -> Told {
        Told { message: spoken.portuguese.clone(), spoken: Some(spoken) }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApiError {
    pub status: StatusCode,
    pub code: &'static str,
    pub message: String,
    pub spoken: Option<Spoken>,
}

pub type ApiResult<T> = Result<T, ApiError>;

impl ApiError {
    pub fn new(status: StatusCode, code: &'static str, message: impl Into<Told>) -> ApiError {
        let told = message.into();
        ApiError { status, code, message: told.message, spoken: told.spoken }
    }

    pub fn bad(message: impl Into<Told>) -> ApiError {
        ApiError::new(StatusCode::BAD_REQUEST, "validation", message)
    }

    pub fn missing(message: impl Into<Told>) -> ApiError {
        ApiError::new(StatusCode::NOT_FOUND, "not_found", message)
    }

    pub fn busy(message: impl Into<Told>) -> ApiError {
        ApiError::new(StatusCode::CONFLICT, "busy", message)
    }

    pub fn told(&self, language: Language) -> String {
        self.spoken.as_ref().map_or_else(|| self.message.clone(), |spoken| spoken.in_language(language))
    }

    fn answer(&self, message: &str) -> Response {
        (self.status, Json(json!({ "error": { "code": self.code, "message": message } }))).into_response()
    }

    pub fn failed(message: impl ToString) -> ApiError {
        ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "failed", message.to_string())
    }

    pub fn upstream(message: impl ToString) -> ApiError {
        ApiError::new(StatusCode::BAD_GATEWAY, "upstream", message.to_string())
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let mut response = self.answer(&self.message);
        if self.spoken.is_some() {
            response.extensions_mut().insert(self);
        }
        response
    }
}

pub async fn localized(State(ctx): State<Ctx>, request: Request, next: Next) -> Response {
    let mut response = next.run(request).await;
    match response.extensions_mut().remove::<ApiError>() {
        Some(error) => error.answer(&error.told(ctx.backend.store.language().unwrap_or_default())),
        None => response,
    }
}

impl From<orchestrator::Error> for ApiError {
    fn from(error: orchestrator::Error) -> ApiError {
        let text = error.to_string();
        match error {
            orchestrator::Error::NotWaiting { .. } | orchestrator::Error::Ended(_) => ApiError::busy(text),
            orchestrator::Error::UnknownStep(_)
            | orchestrator::Error::UnknownAsk(_)
            | orchestrator::Error::UnknownItem(_)
            | orchestrator::Error::UnknownCloudToken => ApiError::missing(text),
            orchestrator::Error::Cloud(_) => ApiError::upstream(text),
            orchestrator::Error::Branch(_) => ApiError::bad(text),
            orchestrator::Error::Store(_) | orchestrator::Error::Script(_) => ApiError::failed(text),
        }
    }
}

impl From<store::Error> for ApiError {
    fn from(error: store::Error) -> ApiError {
        ApiError::failed(error)
    }
}

impl From<connectors::ActionError> for ApiError {
    fn from(error: connectors::ActionError) -> ApiError {
        let code = if error.needs_reconnect() { "reconnect" } else { "upstream" };
        ApiError::new(StatusCode::BAD_GATEWAY, code, error.shown().to_string())
    }
}

pub fn json(value: impl Serialize) -> Response {
    Json(value).into_response()
}

pub fn done() -> Response {
    StatusCode::NO_CONTENT.into_response()
}
