use axum::extract::{Path as Route, State};
use axum::response::{IntoResponse, Response};
use axum::{Extension, Json};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use domain::principal::Principal;
use platform::passkey::{self as webauthn, Authenticator};
use serde::Deserialize;
use serde_json::{Value, json};
use store::Passkey;

use super::settings::{self, Method};
use super::session;
use crate::error::{ApiError, ApiResult, done, json as reply};
use crate::state::{Ctx, Shared};


const SIGN_IN: &str = "passkey.get";
const ENROLL: &str = "passkey.create";
const LASTS: i64 = 300;
const TIMEOUT_MS: i64 = 120_000;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Assertion {
    challenge: String,
    id: String,
    client_data_json: String,
    authenticator_data: String,
    signature: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Attestation {
    challenge: String,
    id: String,
    client_data_json: String,
    authenticator_data: String,
    public_key: String,
    public_key_algorithm: i64,
    #[serde(default)]
    name: String,
}

fn refused() -> ApiError {
    ApiError::bad(crate::error::said(
        "This passkey could not be checked.",
        "No se pudo comprobar esta llave de acceso.",
        "Não foi possível conferir essa chave de acesso.",
    ))
}

fn decoded(text: &str) -> ApiResult<Vec<u8>> {
    URL_SAFE_NO_PAD.decode(text.trim_end_matches('=')).map_err(|_| refused())
}

fn rp_id(shared: &Shared) -> String {
    settings::host_of(&settings::public_origin(shared.port))
}

fn challenged(shared: &Shared, kind: &str, data: &str) -> ApiResult<(String, String)> {
    let id = session::random_text(16)?;
    let challenge = session::random_text(32)?;
    let kept = json!({ "challenge": challenge, "data": data }).to_string();
    shared.backend.store.put_challenge(&id, kind, &kept, settings::now() + LASTS)?;
    Ok((id, challenge))
}

fn taken(shared: &Shared, id: &str, kind: &str) -> ApiResult<(String, String)> {
    let kept = shared.backend.store.take_challenge(id, kind, settings::now())?.ok_or_else(refused)?;
    let kept: Value = serde_json::from_str(&kept).map_err(ApiError::failed)?;
    let challenge = kept["challenge"].as_str().unwrap_or_default().to_owned();
    Ok((challenge, kept["data"].as_str().unwrap_or_default().to_owned()))
}

fn checked_client(shared: &Shared, raw: &[u8], kind: &str, challenge: &str) -> ApiResult<()> {
    let client: Value = serde_json::from_slice(raw).map_err(|_| refused())?;
    let fine = client["type"].as_str() == Some(kind)
        && client["challenge"].as_str().map(|sent| sent.trim_end_matches('=')) == Some(challenge)
        && client["origin"].as_str() == Some(settings::public_origin(shared.port).as_str());
    if fine { Ok(()) } else { Err(refused()) }
}

fn checked_authenticator(shared: &Shared, raw: &[u8]) -> ApiResult<Authenticator> {
    let parsed = Authenticator::parse(raw).ok_or_else(refused)?;
    let rp_hash = platform::crypto::sha256(rp_id(shared).as_bytes());
    if parsed.rp_id_hash != rp_hash || !parsed.present() {
        return Err(refused());
    }
    Ok(parsed)
}

pub async fn sign_in_options(State(ctx): State<Ctx>) -> ApiResult<Response> {
    if !settings::offers(Method::Passkey) {
        return Err(super::email::closed());
    }
    let (id, challenge) = challenged(&ctx, SIGN_IN, "")?;
    Ok(reply(json!({
        "challengeId": id,
        "publicKey": { "challenge": challenge, "rpId": rp_id(&ctx), "timeout": TIMEOUT_MS, "userVerification": "preferred" },
    })))
}

pub async fn sign_in(State(ctx): State<Ctx>, Json(sent): Json<Assertion>) -> ApiResult<Response> {
    if !settings::offers(Method::Passkey) {
        return Err(super::email::closed());
    }
    let (challenge, _) = taken(&ctx, &sent.challenge, SIGN_IN)?;
    let client = decoded(&sent.client_data_json)?;
    checked_client(&ctx, &client, "webauthn.get", &challenge)?;
    let data = decoded(&sent.authenticator_data)?;
    let found = checked_authenticator(&ctx, &data)?;
    let key = ctx.backend.store.passkey(&sent.id)?.ok_or_else(refused)?;
    let message = webauthn::signed_message(&data, &client);
    if !webauthn::verify(key.algorithm, &key.public_key, &message, &decoded(&sent.signature)?) {
        return Err(refused());
    }
    if (found.counter != 0 || key.counter != 0) && found.counter <= key.counter {
        tracing::warn!(target: "audit", passkey = %key.id, "auth.passkey_cloned");
        return Err(refused());
    }
    ctx.backend.store.used_passkey(&key.id, found.counter, settings::now())?;
    let person = ctx.backend.store.user(&key.user)?.ok_or_else(refused)?;
    let headers = session::signed_in(&ctx, &person)?;
    Ok((headers, reply(&person)).into_response())
}

pub async fn enroll_options(State(ctx): State<Ctx>, Extension(person): Extension<Principal>) -> ApiResult<Response> {
    let (id, challenge) = challenged(&ctx, ENROLL, person.user.as_str())?;
    let excluded: Vec<Value> = ctx
        .backend
        .store
        .passkeys(&person.user)?
        .into_iter()
        .map(|key| json!({ "type": "public-key", "id": key.id }))
        .collect();
    let algorithms: Vec<Value> = webauthn::SUPPORTED.iter().map(|alg| json!({ "type": "public-key", "alg": alg })).collect();
    Ok(reply(json!({
        "challengeId": id,
        "publicKey": {
            "challenge": challenge,
            "rp": { "id": rp_id(&ctx), "name": brand::NAME },
            "user": { "id": URL_SAFE_NO_PAD.encode(person.user.as_str()), "name": person.email, "displayName": if person.name.is_empty() { &person.email } else { &person.name } },
            "pubKeyCredParams": algorithms,
            "excludeCredentials": excluded,
            "authenticatorSelection": { "residentKey": "required", "userVerification": "preferred" },
            "attestation": "none",
            "timeout": TIMEOUT_MS,
        },
    })))
}

pub async fn enroll(
    State(ctx): State<Ctx>,
    Extension(person): Extension<Principal>,
    Json(sent): Json<Attestation>,
) -> ApiResult<Response> {
    let (challenge, owner) = taken(&ctx, &sent.challenge, ENROLL)?;
    if owner != person.user.as_str() || !webauthn::SUPPORTED.contains(&sent.public_key_algorithm) {
        return Err(refused());
    }
    checked_client(&ctx, &decoded(&sent.client_data_json)?, "webauthn.create", &challenge)?;
    let found = checked_authenticator(&ctx, &decoded(&sent.authenticator_data)?)?;
    let public_key = decoded(&sent.public_key)?;
    webauthn::spki_key(&public_key).ok_or_else(refused)?;
    let name = sent.name.trim();
    let key = Passkey {
        id: sent.id.trim_end_matches('=').to_owned(),
        user: person.user.clone(),
        public_key,
        algorithm: sent.public_key_algorithm,
        counter: found.counter,
        name: if name.is_empty() { "Passkey".to_owned() } else { name.chars().take(80).collect() },
        created: settings::now(),
        used: None,
    };
    ctx.backend.store.add_passkey(&key)?;
    tracing::info!(target: "audit", user = %person.user, "auth.passkey_added");
    Ok(done())
}

pub async fn list(State(ctx): State<Ctx>, Extension(person): Extension<Principal>) -> ApiResult<Response> {
    let keys: Vec<Value> = ctx
        .backend
        .store
        .passkeys(&person.user)?
        .into_iter()
        .map(|key| json!({ "id": key.id, "name": key.name, "created": key.created, "used": key.used }))
        .collect();
    Ok(reply(keys))
}

pub async fn remove(
    State(ctx): State<Ctx>,
    Extension(person): Extension<Principal>,
    Route(id): Route<String>,
) -> ApiResult<Response> {
    if !ctx.backend.store.remove_passkey(&person.user, &id)? {
        return Err(ApiError::missing("no such passkey"));
    }
    Ok(done())
}
