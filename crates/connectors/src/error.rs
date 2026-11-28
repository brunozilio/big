use std::fmt;
use std::time::Duration;

use serde::{Deserialize, Serialize};

pub const DEFAULT_RETRY_AFTER: Duration = Duration::from_secs(60);
pub const RECONNECT: &str = "the access could not be renewed; reconnect the account";
pub const BLOCKED: &str = "blocked: ";
pub const LOGIN: &str = "login required: ";
pub const SECRET: &str = "a secret was found in the argument ";

pub use domain::effect::ErrorClass;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActionError {
    pub class: ErrorClass,
    pub message: String,
}

impl ActionError {
    pub fn new(class: ErrorClass, message: impl Into<String>) -> ActionError {
        ActionError { class, message: message.into() }
    }

    pub fn transient(message: impl Into<String>) -> ActionError {
        ActionError::new(ErrorClass::Transient, message)
    }

    pub fn auth(message: impl Into<String>) -> ActionError {
        ActionError::new(ErrorClass::Auth, message)
    }

    pub fn conflict(message: impl Into<String>) -> ActionError {
        ActionError::new(ErrorClass::Conflict, message)
    }

    pub fn permanent(message: impl Into<String>) -> ActionError {
        ActionError::new(ErrorClass::Permanent, message)
    }

    pub fn reconnect() -> ActionError {
        ActionError::auth(RECONNECT)
    }

    pub fn login_required(message: impl fmt::Display) -> ActionError {
        ActionError::auth(format!("{LOGIN}{message}"))
    }

    pub fn needs_login(&self) -> bool {
        self.class == ErrorClass::Auth && self.message.starts_with(LOGIN)
    }

    pub fn blocked(reason: impl fmt::Display) -> ActionError {
        ActionError::permanent(format!("{BLOCKED}{reason}"))
    }

    pub fn blocker(&self) -> Option<&str> {
        if self.class == ErrorClass::Permanent { self.message.strip_prefix(BLOCKED) } else { None }
    }

    pub fn shown(&self) -> &str {
        self.blocker().or_else(|| self.message.strip_prefix(LOGIN)).unwrap_or(&self.message)
    }

    pub fn secret_in(field: &str) -> ActionError {
        ActionError::permanent(format!("{SECRET}{field}; the action did not run. Leave the secret out, or declare the field in secretFields if it is meant to carry one"))
    }

    pub fn secret_field(&self) -> Option<&str> {
        let rest = self.message.strip_prefix(SECRET)?;
        rest.split_once(';').map(|(field, _)| field)
    }

    pub fn rate_limited(retry_after: Duration, message: impl Into<String>) -> ActionError {
        ActionError::new(ErrorClass::RateLimited { retry_after }, message)
    }

    pub fn decode(what: impl fmt::Display) -> ActionError {
        ActionError::permanent(format!("the answer could not be read: {what}"))
    }

    pub fn missing(argument: &str) -> ActionError {
        ActionError::permanent(format!("the argument `{argument}` is missing or has the wrong type"))
    }

    pub fn retryable(&self) -> bool {
        matches!(self.class, ErrorClass::Transient | ErrorClass::RateLimited { .. })
    }
}

impl fmt::Display for ActionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{:?}: {}", self.class, self.message)
    }
}

impl std::error::Error for ActionError {}

fn header<'a>(headers: &'a [(String, String)], name: &str) -> Option<&'a str> {
    headers.iter().find(|(key, _)| key.eq_ignore_ascii_case(name)).map(|(_, value)| value.trim())
}

pub fn retry_after(headers: &[(String, String)], now_second: i64) -> Option<Duration> {
    if let Some(seconds) = header(headers, "retry-after").and_then(|value| value.parse::<u64>().ok()) {
        return Some(Duration::from_secs(seconds));
    }
    let exhausted = header(headers, "x-ratelimit-remaining").and_then(|value| value.parse::<u64>().ok()) == Some(0);
    let reset = header(headers, "x-ratelimit-reset").and_then(|value| value.parse::<i64>().ok());
    match (exhausted, reset) {
        (true, Some(reset)) => Some(Duration::from_secs(u64::try_from(reset - now_second).unwrap_or(0).max(1))),
        (true, None) => Some(DEFAULT_RETRY_AFTER),
        _ => None,
    }
}

pub fn classify(status: u16, headers: &[(String, String)], now_second: i64) -> Option<ErrorClass> {
    if (200..400).contains(&status) {
        return None;
    }
    let limited = retry_after(headers, now_second);
    Some(match status {
        429 => ErrorClass::RateLimited { retry_after: limited.unwrap_or(DEFAULT_RETRY_AFTER) },
        403 if limited.is_some() => ErrorClass::RateLimited { retry_after: limited.unwrap_or(DEFAULT_RETRY_AFTER) },
        401 | 403 => ErrorClass::Auth,
        409 | 412 => ErrorClass::Conflict,
        408 | 500..=599 => ErrorClass::Transient,
        _ => ErrorClass::Permanent,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn headers(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
        pairs.iter().map(|(name, value)| ((*name).to_owned(), (*value).to_owned())).collect()
    }

    #[test]
    fn success_and_redirects_are_not_errors() {
        assert_eq!(classify(200, &[], 0), None);
        assert_eq!(classify(304, &[], 0), None);
    }

    #[test]
    fn statuses_map_to_their_class() {
        assert_eq!(classify(500, &[], 0), Some(ErrorClass::Transient));
        assert_eq!(classify(503, &[], 0), Some(ErrorClass::Transient));
        assert_eq!(classify(408, &[], 0), Some(ErrorClass::Transient));
        assert_eq!(classify(401, &[], 0), Some(ErrorClass::Auth));
        assert_eq!(classify(403, &[], 0), Some(ErrorClass::Auth));
        assert_eq!(classify(409, &[], 0), Some(ErrorClass::Conflict));
        assert_eq!(classify(412, &[], 0), Some(ErrorClass::Conflict));
        assert_eq!(classify(404, &[], 0), Some(ErrorClass::Permanent));
        assert_eq!(classify(422, &[], 0), Some(ErrorClass::Permanent));
    }

    #[test]
    fn too_many_requests_waits_what_the_server_asks() {
        let limited = classify(429, &headers(&[("Retry-After", "7")]), 0);
        assert_eq!(limited, Some(ErrorClass::RateLimited { retry_after: Duration::from_secs(7) }));
        assert_eq!(classify(429, &[], 0), Some(ErrorClass::RateLimited { retry_after: DEFAULT_RETRY_AFTER }));
    }

    #[test]
    fn an_exhausted_quota_on_forbidden_is_a_rate_limit_until_the_reset() {
        let quota = headers(&[("x-ratelimit-remaining", "0"), ("x-ratelimit-reset", "1030")]);
        assert_eq!(classify(403, &quota, 1000), Some(ErrorClass::RateLimited { retry_after: Duration::from_secs(30) }));
        let spare = headers(&[("x-ratelimit-remaining", "12")]);
        assert_eq!(classify(403, &spare, 1000), Some(ErrorClass::Auth));
    }

    #[test]
    fn the_class_is_the_domain_s_own_and_serializes_by_kind() {
        let error = ActionError::rate_limited(Duration::from_secs(2), "slow down");
        let value = serde_json::to_value(&error).unwrap_or_default();
        assert_eq!(value["class"], serde_json::json!({"kind": "rate_limited", "retry_after": 2000}));
        let back: ActionError = serde_json::from_value(value).unwrap_or_else(|_| ActionError::permanent("x"));
        assert_eq!(back, error);
        let same: domain::effect::ErrorClass = ErrorClass::Auth;
        assert_eq!(same, ErrorClass::Auth);
    }

    #[test]
    fn only_transient_and_rate_limited_are_retried() {
        assert!(ActionError::transient("x").retryable());
        assert!(ActionError::rate_limited(Duration::from_secs(1), "x").retryable());
        assert!(!ActionError::reconnect().retryable());
        assert!(!ActionError::conflict("x").retryable());
    }

    #[test]
    fn a_login_required_error_is_an_auth_error_that_shows_only_the_connector_text() {
        let error = ActionError::login_required("faça login no Helix");
        assert_eq!(error.class, ErrorClass::Auth);
        assert!(error.needs_login());
        assert_eq!(error.shown(), "faça login no Helix");
        assert!(!ActionError::auth("expired").needs_login());
        assert!(!ActionError::reconnect().needs_login());
    }
}
