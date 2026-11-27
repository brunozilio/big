use std::collections::{HashMap, HashSet};
use std::sync::{Mutex, PoisonError};

use axum::Extension;
use axum::body::Body;
use axum::extract::{Path, RawQuery};
use axum::http::StatusCode;
use axum::http::header::{CONTENT_TYPE, LOCATION};
use axum::response::{IntoResponse, Response};
use domain::principal::Principal;
use serde_json::{Value, json};

use crate::auth::settings;

static LISTENING: Mutex<Option<HashSet<u16>>> = Mutex::new(None);

pub const LOOPBACK_ROUTE: &str = "/oauth/loopback/{port}/{*rest}";

fn loopback_host(host: &str) -> bool {
    matches!(host, "127.0.0.1" | "localhost" | "[::1]" | "::1")
}

pub fn loopback(redirect: &str) -> Option<url::Url> {
    let parsed = url::Url::parse(redirect).ok()?;
    (parsed.scheme() == "http" && parsed.host_str().is_some_and(loopback_host)).then_some(parsed)
}

fn redirect_in(authorize: &str) -> Option<url::Url> {
    let parsed = url::Url::parse(authorize).ok()?;
    let redirect = parsed.query_pairs().find(|(key, _)| key == "redirect_uri").map(|(_, value)| value.into_owned())?;
    loopback(&redirect)
}

fn bare(url: &url::Url) -> String {
    let mut bare = url.clone();
    bare.set_query(None);
    bare.set_fragment(None);
    bare.to_string()
}

pub fn for_connector(port: u16, redirect: &str) -> Option<Value> {
    if settings::mode() != settings::Mode::Shared {
        return None;
    }
    let from = loopback(redirect)?;
    Some(json!({ "from": bare(&from), "to": format!("{}/oauth/callback", settings::public_base(port)) }))
}

pub fn for_ai(port: u16, authorize: &str) -> Option<Value> {
    if settings::mode() != settings::Mode::Shared {
        return None;
    }
    let from = redirect_in(authorize)?;
    let listening = from.port_or_known_default()?;
    remember(listening);
    let path = from.path().trim_start_matches('/');
    let to = format!("{}/oauth/loopback/{listening}/{path}", settings::public_base(port));
    Some(json!({ "from": bare(&from), "to": to }))
}

fn remember(port: u16) {
    LISTENING.lock().unwrap_or_else(PoisonError::into_inner).get_or_insert_with(HashSet::new).insert(port);
}

pub fn forget(authorize: &str) {
    if let Some(port) = redirect_in(authorize).and_then(|url| url.port_or_known_default())
        && let Some(ports) = LISTENING.lock().unwrap_or_else(PoisonError::into_inner).as_mut()
    {
        ports.remove(&port);
    }
}

fn listening(port: u16) -> bool {
    LISTENING.lock().unwrap_or_else(PoisonError::into_inner).as_ref().is_some_and(|ports| ports.contains(&port))
}

pub async fn returned(
    Extension(person): Extension<Principal>,
    Path(params): Path<HashMap<String, String>>,
    RawQuery(query): RawQuery,
) -> Response {
    let port = params.get("port").and_then(|port| port.parse::<u16>().ok());
    let Some(port) = port.filter(|port| listening(*port) && crate::scope::is_admin(&person)) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let rest = params.get("rest").map(String::as_str).unwrap_or_default();
    let query = query.map(|query| format!("?{query}")).unwrap_or_default();
    let target = format!("http://127.0.0.1:{port}/{rest}{query}");
    let Ok(client) = reqwest::Client::builder().redirect(reqwest::redirect::Policy::none()).build() else {
        return StatusCode::BAD_GATEWAY.into_response();
    };
    match client.get(&target).send().await {
        Ok(answer) => answered(answer).await,
        Err(error) => (StatusCode::BAD_GATEWAY, error.to_string()).into_response(),
    }
}

async fn answered(answer: reqwest::Response) -> Response {
    let status = StatusCode::from_u16(answer.status().as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
    let kind = answer.headers().get(CONTENT_TYPE.as_str()).and_then(|value| value.to_str().ok()).map(str::to_owned);
    let location = answer.headers().get(LOCATION.as_str()).and_then(|value| value.to_str().ok()).map(str::to_owned);
    let body = answer.bytes().await.unwrap_or_default();
    let mut response = Response::builder().status(status);
    if let Some(kind) = kind {
        response = response.header(CONTENT_TYPE, kind);
    }
    if let Some(location) = location
        .filter(|location| !location.starts_with("http://127.0.0.1") && !location.starts_with("http://localhost"))
    {
        response = response.header(LOCATION, location);
    }
    response.body(Body::from(body)).unwrap_or_else(|_| StatusCode::BAD_GATEWAY.into_response())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_plain_http_loopback_redirect_is_relayed() {
        assert!(loopback("http://127.0.0.1:7420/oauth/callback").is_some());
        assert!(loopback("http://localhost:1455/auth/callback").is_some());
        assert!(loopback("https://127.0.0.1:7420/cb").is_none());
        assert!(loopback("http://app.example.com/oauth/callback").is_none());
        let authorize = "https://idp.example.com/authorize?client_id=x&redirect_uri=http%3A%2F%2Flocalhost%3A1455%2Fauth%2Fcallback&state=s";
        let found = redirect_in(authorize).map(|url| bare(&url));
        assert_eq!(found.as_deref(), Some("http://localhost:1455/auth/callback"));
    }
}
