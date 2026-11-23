use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Serialize;
use serde_json::json;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApiError {
    pub status: StatusCode,
    pub code: &'static str,
    pub message: String,
}

pub type ApiResult<T> = Result<T, ApiError>;

impl ApiError {
    pub fn new(status: StatusCode, code: &'static str, message: impl Into<String>) -> ApiError {
        ApiError { status, code, message: message.into() }
    }

    pub fn bad(message: impl Into<String>) -> ApiError {
        ApiError::new(StatusCode::BAD_REQUEST, "validation", message)
    }

    pub fn missing(message: impl Into<String>) -> ApiError {
        ApiError::new(StatusCode::NOT_FOUND, "not_found", message)
    }

    pub fn busy(message: impl Into<String>) -> ApiError {
        ApiError::new(StatusCode::CONFLICT, "busy", message)
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
        (self.status, Json(json!({ "error": { "code": self.code, "message": self.message } }))).into_response()
    }
}

impl From<orchestrator::Error> for ApiError {
    fn from(error: orchestrator::Error) -> ApiError {
        let text = error.to_string();
        match error {
            orchestrator::Error::NotWaiting { .. } | orchestrator::Error::Ended(_) => ApiError::busy(text),
            orchestrator::Error::UnknownStep(_)
            | orchestrator::Error::UnknownAsk(_)
            | orchestrator::Error::UnknownItem(_) => ApiError::missing(text),
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
        ApiError::upstream(error.shown())
    }
}

pub fn json(value: impl Serialize) -> Response {
    Json(value).into_response()
}

pub fn done() -> Response {
    StatusCode::NO_CONTENT.into_response()
}
