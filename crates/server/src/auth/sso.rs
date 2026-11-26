use std::collections::HashMap;

use axum::extract::{Query, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use domain::principal::{Principal, UserId, domain_of, normalized_email};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::settings::{self, Method};
use super::session;
use crate::error::{ApiError, ApiResult};
use crate::state::{Ctx, Shared};

const KIND: &str = "sso";
const LASTS: i64 = 600;
pub const CALLBACK: &str = "/api/auth/sso/callback";

#[derive(Debug, Deserialize)]
struct Discovery {
    issuer: String,
    authorization_endpoint: String,
    token_endpoint: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct Pending {
    verifier: String,
    nonce: String,
    next: String,
}

#[derive(Debug, Deserialize)]
struct Tokens {
    id_token: String,
}

fn client() -> ApiResult<reqwest::Client> {
    reqwest::Client::builder().build().map_err(ApiError::failed)
}

async fn discovery() -> ApiResult<Discovery> {
    let issuer = brand::auth::SSO_ISSUER.trim().trim_end_matches('/');
    let url = format!("{issuer}/.well-known/openid-configuration");
    let response = client()?.get(url).send().await.map_err(ApiError::upstream)?;
    let body = response.error_for_status().map_err(ApiError::upstream)?.bytes().await.map_err(ApiError::upstream)?;
    serde_json::from_slice(&body).map_err(ApiError::upstream)
}

fn redirect_uri(shared: &Shared) -> String {
    format!("{}{CALLBACK}", settings::public_origin(shared.port))
}

pub fn safe_next(next: Option<&String>) -> String {
    next.filter(|path| path.starts_with('/') && !path.starts_with("//") && !path.contains('\\'))
        .cloned()
        .unwrap_or_else(|| "/".to_owned())
}

fn moved(to: &str, extra: Option<HeaderMap>) -> Response {
    let mut headers = extra.unwrap_or_default();
    if let Ok(value) = HeaderValue::from_str(to) {
        headers.insert(header::LOCATION, value);
    }
    (StatusCode::FOUND, headers).into_response()
}

fn failed(reason: &str) -> Response {
    tracing::warn!(target: "audit", reason, "auth.sso_failed");
    moved("/login?error=sso", None)
}

pub async fn start(State(ctx): State<Ctx>, Query(query): Query<HashMap<String, String>>) -> Response {
    match begin(&ctx, safe_next(query.get("next"))).await {
        Ok(url) => moved(&url, None),
        Err(error) => failed(&error.message),
    }
}

async fn begin(shared: &Shared, next: String) -> ApiResult<String> {
    if !settings::offers(Method::Sso) {
        return Err(super::email::closed());
    }
    let found = discovery().await?;
    let pkce = platform::crypto::pkce().map_err(ApiError::failed)?;
    let state = session::random_text(24)?;
    let nonce = session::random_text(24)?;
    let pending = Pending { verifier: pkce.verifier, nonce: nonce.clone(), next };
    let data = serde_json::to_string(&pending).map_err(ApiError::failed)?;
    shared.backend.store.put_challenge(&state, KIND, &data, settings::now() + LASTS)?;
    let mut url = url::Url::parse(&found.authorization_endpoint).map_err(ApiError::upstream)?;
    url.query_pairs_mut()
        .append_pair("response_type", "code")
        .append_pair("client_id", brand::auth::SSO_CLIENT_ID.trim())
        .append_pair("redirect_uri", &redirect_uri(shared))
        .append_pair("scope", brand::auth::SSO_SCOPES.trim())
        .append_pair("state", &state)
        .append_pair("nonce", &nonce)
        .append_pair("code_challenge", &pkce.challenge)
        .append_pair("code_challenge_method", "S256");
    Ok(url.into())
}

pub async fn callback(State(ctx): State<Ctx>, Query(query): Query<HashMap<String, String>>) -> Response {
    let (Some(code), Some(state)) = (query.get("code"), query.get("state")) else {
        return failed(query.get("error").map_or("no code", String::as_str));
    };
    match finish(&ctx, code, state).await {
        Ok((headers, next)) => moved(&next, Some(headers)),
        Err(error) => failed(&error.message),
    }
}

async fn finish(shared: &Shared, code: &str, state: &str) -> ApiResult<(HeaderMap, String)> {
    let data = shared
        .backend
        .store
        .take_challenge(state, KIND, settings::now())?
        .ok_or_else(|| ApiError::bad("the sign-in took too long or was already used"))?;
    let pending: Pending = serde_json::from_str(&data).map_err(ApiError::failed)?;
    let found = discovery().await?;
    let tokens = exchange(shared, &found, code, &pending.verifier).await?;
    let claims = claims(&tokens.id_token)?;
    let person = person(&claims, &found.issuer, &pending.nonce)?;
    if !settings::allowed_domains().is_empty() && !super::email::admitted(shared, &person.email)? {
        return Err(ApiError::bad(format!("{} is not allowed", domain_of(&person.email))));
    }
    let person = shared.backend.store.ensure_user(&person, settings::now())?;
    Ok((session::signed_in(shared, &person)?, pending.next))
}

async fn exchange(shared: &Shared, found: &Discovery, code: &str, verifier: &str) -> ApiResult<Tokens> {
    let redirect = redirect_uri(shared);
    let mut form = vec![
        ("grant_type", "authorization_code"),
        ("code", code),
        ("redirect_uri", redirect.as_str()),
        ("client_id", brand::auth::SSO_CLIENT_ID.trim()),
        ("code_verifier", verifier),
    ];
    let secret = brand::auth::SSO_CLIENT_SECRET.trim();
    if !secret.is_empty() {
        form.push(("client_secret", secret));
    }
    let response = client()?.post(&found.token_endpoint).form(&form).send().await.map_err(ApiError::upstream)?;
    let body = response.error_for_status().map_err(ApiError::upstream)?.bytes().await.map_err(ApiError::upstream)?;
    serde_json::from_slice(&body).map_err(ApiError::upstream)
}

pub fn claims(id_token: &str) -> ApiResult<Value> {
    let payload = id_token.split('.').nth(1).ok_or_else(|| ApiError::upstream("the id token has no payload"))?;
    let bytes = URL_SAFE_NO_PAD.decode(payload.trim_end_matches('=')).map_err(ApiError::upstream)?;
    serde_json::from_slice(&bytes).map_err(ApiError::upstream)
}

fn audience_has(claims: &Value, client: &str) -> bool {
    match &claims["aud"] {
        Value::String(one) => one == client,
        Value::Array(many) => many.iter().any(|aud| aud.as_str() == Some(client)),
        _ => false,
    }
}

pub fn person(claims: &Value, issuer: &str, nonce: &str) -> ApiResult<Principal> {
    let fine = claims["iss"].as_str() == Some(issuer)
        && audience_has(claims, brand::auth::SSO_CLIENT_ID.trim())
        && claims["nonce"].as_str() == Some(nonce)
        && claims["exp"].as_i64().is_some_and(|exp| exp > settings::now())
        && claims["email_verified"].as_bool() != Some(false);
    if !fine {
        return Err(ApiError::upstream("the id token does not match this sign-in"));
    }
    let email = claims["email"]
        .as_str()
        .or_else(|| claims["preferred_username"].as_str())
        .and_then(normalized_email)
        .ok_or_else(|| ApiError::upstream("the id token has no email"))?;
    let name = claims["name"].as_str().unwrap_or_default().to_owned();
    Ok(Principal { user: UserId(format!("u-{}", uuid::Uuid::new_v4().simple())), email, name })
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn only_a_local_path_is_followed_after_signing_in() {
        assert_eq!(safe_next(Some(&"/items/1".to_owned())), "/items/1");
        assert_eq!(safe_next(Some(&"//evil.com".to_owned())), "/");
        assert_eq!(safe_next(Some(&"https://evil.com".to_owned())), "/");
        assert_eq!(safe_next(None), "/");
    }

    #[test]
    fn the_id_token_payload_is_read() {
        let payload = URL_SAFE_NO_PAD.encode(br#"{"email":"a@b.co"}"#);
        let read = claims(&format!("h.{payload}.s"));
        assert_eq!(read.ok(), Some(json!({ "email": "a@b.co" })));
    }

    #[test]
    fn a_token_from_another_issuer_or_nonce_is_refused() {
        let token = json!({ "iss": "https://idp", "aud": brand::auth::SSO_CLIENT_ID.trim(), "nonce": "n", "exp": i64::MAX, "email": "Ana@B.co" });
        assert_eq!(person(&token, "https://idp", "n").map(|p| p.email).ok(), Some("ana@b.co".to_owned()));
        assert!(person(&token, "https://other", "n").is_err());
        assert!(person(&token, "https://idp", "x").is_err());
    }
}
