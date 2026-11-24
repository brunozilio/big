mod device;
mod loopback;
pub mod mcp;
mod tokens;

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use jiff::Timestamp;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::clock::after_seconds;
use crate::error::ActionError;
use crate::http::HttpResponse;

pub use device::{DeviceApp, DeviceCode, Dialect, Polled, RegisteredClient, SsoSpec, register_client};
pub use loopback::{Authorization, Loopback, LoopbackSignIn};
pub use tokens::{MemoryTokenStore, REFRESH_WINDOW_SECONDS, TokenKeeper, TokenStore};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Grant {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_at: Option<Timestamp>,
    pub scope: Option<String>,
    pub client: Option<RegisteredClient>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub method: Option<String>,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub extra: Value,
}

impl Grant {
    pub fn bearer(access_token: &str) -> Grant {
        Grant {
            access_token: access_token.to_owned(),
            refresh_token: None,
            expires_at: None,
            scope: None,
            client: None,
            method: None,
            extra: Value::Null,
        }
    }

    pub fn stale(&self, now: Timestamp) -> bool {
        self.expires_at.is_some_and(|at| at.as_second() - now.as_second() <= REFRESH_WINDOW_SECONDS)
    }
}

pub fn random_token() -> Result<String, ActionError> {
    let mut bytes = [0_u8; 32];
    getrandom::fill(&mut bytes).map_err(|error| ActionError::permanent(format!("no randomness: {error}")))?;
    Ok(URL_SAFE_NO_PAD.encode(bytes))
}

pub fn challenge(verifier: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
}

pub(crate) fn text(answer: &Value, names: &[&str]) -> Option<String> {
    names.iter().find_map(|name| answer.get(*name).and_then(Value::as_str)).map(str::to_owned)
}

pub(crate) fn number(answer: &Value, names: &[&str]) -> Option<i64> {
    names.iter().find_map(|name| answer.get(*name).and_then(Value::as_i64))
}

fn error_code(answer: &Value) -> Option<String> {
    text(answer, &["error", "__type"])
        .map(|code| code.rsplit('#').next().unwrap_or_default().to_ascii_lowercase().replace('_', ""))
}

fn refused(code: &str, answer: &Value) -> ActionError {
    let description = text(answer, &["error_description", "message", "Message"]);
    ActionError::auth(description.map_or_else(|| code.to_owned(), |text| format!("{code}: {text}")))
}

pub(crate) fn token_error(response: &HttpResponse) -> Option<ActionError> {
    let answer = response.value();
    if let Some(code) = error_code(&answer) {
        return Some(refused(&code, &answer));
    }
    response.clone().checked().err()
}

pub(crate) fn granted(answer: &Value, now: Timestamp) -> Result<Grant, ActionError> {
    let access_token = text(answer, &["access_token", "accessToken"])
        .ok_or_else(|| ActionError::auth("the answer has no access token"))?;
    Ok(Grant {
        access_token,
        refresh_token: text(answer, &["refresh_token", "refreshToken"]),
        expires_at: number(answer, &["expires_in", "expiresIn"]).map(|seconds| after_seconds(now, seconds)),
        scope: text(answer, &["scope"]),
        client: None,
        method: None,
        extra: Value::Null,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_pkce_challenge_is_the_sha256_of_the_verifier() {
        assert_eq!(
            challenge("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
        let token = random_token().unwrap_or_default();
        assert_eq!(token.len(), 43);
        assert_ne!(Some(token), random_token().ok());
    }

    #[test]
    fn a_grant_is_stale_five_minutes_before_it_expires() {
        let now = Timestamp::from_second(1_000).unwrap_or(Timestamp::UNIX_EPOCH);
        let mut grant = Grant::bearer("t");
        assert!(!grant.stale(now));
        grant.expires_at = Some(after_seconds(now, 301));
        assert!(!grant.stale(now));
        grant.expires_at = Some(after_seconds(now, 300));
        assert!(grant.stale(now));
    }
}
