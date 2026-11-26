use axum::http::header::{COOKIE, SET_COOKIE};
use axum::http::{HeaderMap, HeaderValue};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use domain::principal::Principal;

use super::settings;
use crate::error::{ApiError, ApiResult};
use crate::state::Shared;

pub fn cookie_name() -> String {
    format!("{}_session", brand::COMMAND.replace('-', "_"))
}

pub fn random_text(bytes: usize) -> ApiResult<String> {
    Ok(URL_SAFE_NO_PAD.encode(platform::crypto::random(bytes).map_err(ApiError::failed)?))
}

pub fn hashed(secret: &str) -> String {
    platform::crypto::hex(&platform::crypto::sha256(secret.as_bytes()))
}

pub fn token_in(headers: &HeaderMap) -> Option<String> {
    let name = cookie_name();
    headers
        .get_all(COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(';'))
        .filter_map(|pair| pair.trim().split_once('='))
        .find(|(key, _)| *key == name)
        .map(|(_, value)| value.to_owned())
        .filter(|value| !value.is_empty())
}

fn secure(shared: &Shared) -> bool {
    settings::public_origin(shared.port).starts_with("https://")
}

pub fn cookie(shared: &Shared, token: &str, seconds: i64) -> String {
    let secure = if secure(shared) { "; Secure" } else { "" };
    format!("{}={token}; Path=/; HttpOnly; SameSite=Lax; Max-Age={seconds}{secure}", cookie_name())
}

pub fn signed_in(shared: &Shared, person: &Principal) -> ApiResult<HeaderMap> {
    let token = random_text(32)?;
    let now = settings::now();
    let seconds = settings::session_seconds();
    shared.backend.store.forget_expired(now)?;
    shared.backend.store.open_session(&hashed(&token), &person.user, now, now + seconds)?;
    tracing::info!(target: "audit", user = %person.user, "auth.signed_in");
    let mut headers = HeaderMap::new();
    let value = HeaderValue::from_str(&cookie(shared, &token, seconds)).map_err(ApiError::failed)?;
    headers.insert(SET_COOKIE, value);
    Ok(headers)
}

pub fn signed_out(shared: &Shared, headers: &HeaderMap) -> ApiResult<HeaderMap> {
    if let Some(token) = token_in(headers) {
        shared.backend.store.close_session(&hashed(&token))?;
    }
    let mut out = HeaderMap::new();
    out.insert(SET_COOKIE, HeaderValue::from_str(&cookie(shared, "", 0)).map_err(ApiError::failed)?);
    Ok(out)
}

pub fn person(shared: &Shared, headers: &HeaderMap) -> ApiResult<Option<Principal>> {
    match settings::mode() {
        settings::Mode::Local => Ok(Some(Principal::local(brand::auth::LOCAL_EMAIL.trim()))),
        settings::Mode::Shared => match token_in(headers) {
            Some(token) => Ok(shared.backend.store.session_user(&hashed(&token), settings::now())?),
            None => Ok(None),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_session_cookie_is_found_among_others() {
        let mut headers = HeaderMap::new();
        let line = format!("a=1; {}=tok; b=2", cookie_name());
        headers.insert(COOKIE, HeaderValue::from_str(&line).unwrap_or(HeaderValue::from_static("")));
        assert_eq!(token_in(&headers), Some("tok".to_owned()));
        assert_eq!(token_in(&HeaderMap::new()), None);
    }
}
