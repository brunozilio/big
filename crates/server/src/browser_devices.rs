use std::collections::HashMap;
use std::convert::Infallible;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use axum::Extension;
use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::header::{AUTHORIZATION, CONTENT_DISPOSITION, CONTENT_TYPE};
use axum::http::{HeaderMap, StatusCode};
use axum::response::sse::{Event, Sse};
use axum::response::{IntoResponse, Response};
use domain::principal::Principal;
use futures_util::StreamExt;
use serde::Deserialize;
use serde_json::{Value, json};
use store::BrowserDevice;

use crate::auth::session::{hashed, random_text};
use crate::auth::settings;
use crate::error::{ApiError, ApiResult, json as reply, said};
use crate::state::{Ctx, held};

pub const PAIRING: &str = "/api/browser/pairing";
pub const CLAIM: &str = "/api/browser/claim";
pub const STREAM: &str = "/api/browser/stream";
pub const REPLY: &str = "/api/browser/reply";
pub const PACKAGE: &str = "/api/browser/extension.zip";
pub const DEVICES: &str = "/api/browser/devices";
pub const DEVICE: &str = "/api/browser/devices/{device}";

const CODE_LASTS: Duration = Duration::from_secs(300);
const TOKEN_BYTES: usize = 32;
const CODE_BYTES: usize = 24;

static CODES: Mutex<Vec<(String, String, Instant)>> = Mutex::new(Vec::new());
static PACKAGED: std::sync::OnceLock<Result<browser::Packaged, String>> = std::sync::OnceLock::new();

pub fn packaged() -> Result<&'static browser::Packaged, String> {
    PACKAGED.get_or_init(browser::packaged).as_ref().map_err(Clone::clone)
}

pub fn build() -> Option<&'static str> {
    packaged().ok().map(|made| made.build.as_str())
}

pub const OWNER_PREFIX: &str = "owner:";

pub fn of_owner(owner: &str) -> String {
    format!("{OWNER_PREFIX}{owner}")
}

pub fn open_path(path: &str) -> bool {
    path == CLAIM || path == STREAM || path == REPLY
}

fn issue(owner: &str) -> ApiResult<String> {
    let code = random_text(CODE_BYTES)?;
    let mut codes = held(&CODES);
    codes.retain(|(_, _, born)| born.elapsed() < CODE_LASTS);
    codes.push((hashed(&code), owner.to_owned(), Instant::now()));
    Ok(code)
}

fn redeem(code: &str) -> Option<String> {
    let wanted = hashed(code);
    let mut codes = held(&CODES);
    codes.retain(|(_, _, born)| born.elapsed() < CODE_LASTS);
    let found = codes.iter().position(|(kept, _, _)| *kept == wanted)?;
    Some(codes.remove(found).1)
}

fn refused() -> ApiError {
    ApiError::new(
        StatusCode::UNAUTHORIZED,
        "browser_unpaired",
        said(
            "This browser is not paired. Connect it again from the app.",
            "Este navegador no está vinculado. Conéctalo de nuevo desde la app.",
            "Este navegador não está pareado. Conecte de novo pelo app.",
        ),
    )
}

fn paired(ctx: &Ctx, headers: &HeaderMap) -> ApiResult<BrowserDevice> {
    let token = headers
        .get(AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .map(str::trim)
        .filter(|token| !token.is_empty())
        .ok_or_else(refused)?;
    ctx.backend.store.browser_device(&hashed(token))?.ok_or_else(refused)
}

pub async fn pairing(State(ctx): State<Ctx>, Extension(person): Extension<Principal>) -> ApiResult<Response> {
    let code = issue(person.owner())?;
    Ok(reply(json!({ "code": code, "server": settings::public_base(ctx.port) })))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Claim {
    code: String,
    device_id: String,
    #[serde(default)]
    name: String,
}

pub async fn claim(State(ctx): State<Ctx>, Json(claim): Json<Claim>) -> ApiResult<Response> {
    let device = claim.device_id.trim();
    if device.is_empty() || device.len() > 100 {
        return Err(ApiError::bad(said("Send the deviceId.", "Envía el deviceId.", "Informe o deviceId.")));
    }
    let owner = redeem(claim.code.trim()).ok_or_else(|| {
        ApiError::new(
            StatusCode::FORBIDDEN,
            "pairing_expired",
            said(
                "This pairing code expired. Click Connect in the app again.",
                "Este código de vinculación venció. Haz clic en Conectar en la app de nuevo.",
                "Este código de pareamento venceu. Clique em Conectar no app de novo.",
            ),
        )
    })?;
    let token = random_text(TOKEN_BYTES)?;
    let name: String = claim.name.trim().chars().take(80).collect();
    let name = if name.is_empty() { "Chrome".to_owned() } else { name };
    let kept = BrowserDevice { device: device.to_owned(), owner, name, created: settings::now() };
    ctx.backend.store.add_browser_device(&hashed(&token), &kept)?;
    tracing::info!(device = %kept.device, "a browser was paired");
    Ok(reply(json!({ "token": token, "server": settings::public_base(ctx.port) })))
}

pub async fn stream(
    State(ctx): State<Ctx>,
    headers: HeaderMap,
    Query(sent): Query<HashMap<String, String>>,
) -> ApiResult<Response> {
    let device = paired(&ctx, &headers)?;
    let mut hello = json!(sent);
    hello["deviceId"] = json!(device.device);
    if hello["name"].as_str().is_none_or(str::is_empty) {
        hello["name"] = json!(device.name);
    }
    let (info, build) = browser::hello(&hello).ok_or_else(refused)?;
    let told = browser::open(&ctx.backend.browser, info, &build, Some(device.owner))
        .map(|text| Ok::<Event, Infallible>(Event::default().data(text)));
    Ok(Sse::new(told).into_response())
}

pub async fn answer(State(ctx): State<Ctx>, headers: HeaderMap, body: String) -> ApiResult<Response> {
    let device = paired(&ctx, &headers)?;
    browser::reply(&ctx.backend.browser, &device.device, &body);
    Ok(StatusCode::NO_CONTENT.into_response())
}

pub async fn package() -> ApiResult<Response> {
    let made = packaged().map_err(ApiError::failed)?;
    let disposition = format!("attachment; filename=\"{}\"", made.name);
    Ok(([(CONTENT_TYPE, "application/zip".to_owned()), (CONTENT_DISPOSITION, disposition)], made.bytes.clone())
        .into_response())
}

pub fn devices_of(ctx: &Ctx, owner: &str) -> ApiResult<Value> {
    let connected = ctx.backend.browser.owned_by(owner);
    let kept = ctx.backend.store.browser_devices_of(owner)?;
    let listed: Vec<Value> = kept
        .iter()
        .map(|device| {
            let live = connected.iter().find(|browser| browser.device_id == device.device);
            json!({
                "device": device.device,
                "name": live.map_or(device.name.as_str(), |browser| browser.name.as_str()),
                "created": device.created,
                "connected": live.is_some(),
            })
        })
        .collect();
    Ok(json!(listed))
}

pub async fn devices(State(ctx): State<Ctx>, Extension(person): Extension<Principal>) -> ApiResult<Response> {
    Ok(reply(devices_of(&ctx, person.owner())?))
}

pub async fn forget(
    State(ctx): State<Ctx>,
    Extension(person): Extension<Principal>,
    Path(device): Path<String>,
) -> ApiResult<Response> {
    let owner = person.owner();
    if !ctx.backend.store.forget_browser_device(owner, &device)? {
        return Err(ApiError::missing(said(
            "There is no such browser.",
            "No existe ese navegador.",
            "Não existe esse navegador.",
        )));
    }
    ctx.backend.browser.unlink_device(owner, &device);
    Ok(StatusCode::NO_CONTENT.into_response())
}

pub fn scope(ctx: &Ctx) {
    if !crate::scope::shared_mode() {
        return;
    }
    let weak = std::sync::Arc::downgrade(ctx);
    ctx.backend.browser.scoped(std::sync::Arc::new(move |key: &str| {
        if let Some(owner) = key.strip_prefix(OWNER_PREFIX) {
            return Some(owner.to_owned());
        }
        let shared = weak.upgrade()?;
        match key.strip_prefix(crate::connector_browser::CONNECTOR_PREFIX) {
            Some(connection) => shared.backend.store.owner_of(store::ownership::CONNECTION, connection).ok(),
            None => crate::scope::session_owner(&shared, key),
        }
    }));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pairing_code_is_used_once() {
        let code = issue("ana").unwrap_or_default();
        assert_eq!(redeem("wrong"), None);
        assert_eq!(redeem(&code), Some("ana".to_owned()));
        assert_eq!(redeem(&code), None);
    }

    #[test]
    fn only_the_browser_routes_that_carry_their_own_proof_are_open() {
        assert!(open_path(CLAIM) && open_path(STREAM) && open_path(REPLY));
        assert!(!open_path(PAIRING) && !open_path(DEVICES) && !open_path(PACKAGE));
    }
}
