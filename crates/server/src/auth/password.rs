use std::sync::OnceLock;

use axum::extract::{Path, State};
use axum::response::{IntoResponse, Response};
use axum::{Extension, Json};
use domain::principal::{Principal, UserId, normalized_login};
use serde::Deserialize;
use serde_json::json;
use store::Store;

use super::email::closed;
use super::session;
use super::settings::{self, Method};
use crate::error::{ApiError, ApiResult, done, json as reply, said};
use crate::state::Ctx;

pub const SHORTEST: usize = 8;
const BOOTSTRAPPED: &str = "auth.bootstrap_admin";

#[derive(Debug, Deserialize)]
pub struct Entered {
    login: String,
    password: String,
}

#[derive(Debug, Deserialize)]
pub struct Changed {
    #[serde(default)]
    current: String,
    password: String,
}

#[derive(Debug, Deserialize)]
pub struct Added {
    login: String,
    #[serde(default)]
    name: String,
    password: String,
}

#[derive(Debug, Deserialize)]
pub struct Reset {
    password: String,
}

#[derive(Debug, Deserialize)]
struct Bootstrap {
    login: String,
    password: String,
}

fn refused() -> ApiError {
    ApiError::bad(said("Wrong user or password.", "Usuario o contraseña incorrectos.", "Usuário ou senha incorretos."))
}

fn wrong_current() -> ApiError {
    ApiError::bad(said(
        "The current password is wrong.",
        "La contraseña actual es incorrecta.",
        "A senha atual está errada.",
    ))
}

fn locked() -> ApiError {
    ApiError::bad(said(
        "Too many wrong tries. Wait a few minutes and try again.",
        "Demasiados intentos fallidos. Espera unos minutos y vuelve a intentarlo.",
        "Tentativas erradas demais. Espere alguns minutos e tente de novo.",
    ))
}

fn too_short() -> ApiError {
    ApiError::bad(said(
        format!("The password needs at least {SHORTEST} characters."),
        format!("La contraseña necesita al menos {SHORTEST} caracteres."),
        format!("A senha precisa de pelo menos {SHORTEST} caracteres."),
    ))
}

fn bad_login() -> ApiError {
    ApiError::bad(said(
        "Use up to 64 letters, digits, dots, dashes, underscores or @ in the user name.",
        "Usa hasta 64 letras, dígitos, puntos, guiones, guiones bajos o @ en el usuario.",
        "Use até 64 letras, dígitos, pontos, hífens, sublinhados ou @ no usuário.",
    ))
}

fn taken() -> ApiError {
    ApiError::bad(said("That user name is taken.", "Ese usuario ya existe.", "Esse usuário já existe."))
}

fn missing() -> ApiError {
    ApiError::missing(said("This person does not exist.", "Esta persona no existe.", "Essa pessoa não existe."))
}

fn hashed(password: &str) -> ApiResult<String> {
    if password.chars().count() < SHORTEST {
        return Err(too_short());
    }
    platform::password::hash(password).map_err(ApiError::failed)
}

fn decoy() -> &'static str {
    static DECOY: OnceLock<String> = OnceLock::new();
    DECOY.get_or_init(|| platform::password::hash("decoy password").unwrap_or_default())
}

async fn matches(stored: Option<String>, password: String) -> bool {
    let compared = tokio::task::spawn_blocking(move || {
        let found = stored.is_some();
        let checked = platform::password::matches(stored.as_deref().unwrap_or_else(|| decoy()), &password);
        found && checked
    });
    compared.await.unwrap_or(false)
}

pub async fn sign_in(State(ctx): State<Ctx>, Json(entered): Json<Entered>) -> ApiResult<Response> {
    if !settings::offers(Method::Password) {
        return Err(closed());
    }
    let login = normalized_login(&entered.login).ok_or_else(refused)?;
    let store = &ctx.backend.store;
    let now = settings::now();
    if store.login_locked(&login, now)? {
        tracing::warn!(target: "audit", "auth.login_locked");
        return Err(locked());
    }
    let person = store.user_by_email(&login)?;
    let stored = match &person {
        Some(person) => store.password_of(&person.user)?,
        None => None,
    };
    let Some(person) = person else {
        let _ = matches(None, entered.password).await;
        store.login_failed(&login, now, settings::login_attempts(), settings::lock_seconds())?;
        return Err(refused());
    };
    if !matches(stored, entered.password).await {
        store.login_failed(&login, now, settings::login_attempts(), settings::lock_seconds())?;
        tracing::warn!(target: "audit", user = %person.user, "auth.password_refused");
        return Err(refused());
    }
    store.login_succeeded(&login)?;
    let person = store.ensure_user(&person, now)?;
    let headers = session::signed_in(&ctx, &person)?;
    Ok((headers, reply(&person)).into_response())
}

pub async fn change(
    State(ctx): State<Ctx>,
    Extension(person): Extension<Principal>,
    Json(changed): Json<Changed>,
) -> ApiResult<Response> {
    let store = &ctx.backend.store;
    let stored = store.password_of(&person.user)?;
    if stored.is_some() && !matches(stored, changed.current).await {
        return Err(wrong_current());
    }
    store.set_password(&person.user, &hashed(&changed.password)?, settings::now())?;
    tracing::info!(target: "audit", user = %person.user, "auth.password_changed");
    Ok(done())
}

pub async fn add_person(State(ctx): State<Ctx>, Json(added): Json<Added>) -> ApiResult<Response> {
    let login = normalized_login(&added.login).ok_or_else(bad_login)?;
    let hash = hashed(&added.password)?;
    let user = UserId(format!("u-{}", uuid::Uuid::new_v4().simple()));
    let wanted = Principal { user: user.clone(), email: login, name: added.name.trim().to_owned() };
    let store = &ctx.backend.store;
    if store.user_by_email(&wanted.email)?.is_some() || !store.add_user(&wanted, settings::now())? {
        return Err(taken());
    }
    store.set_password(&user, &hash, settings::now())?;
    tracing::info!(target: "audit", user = %user, "auth.person_added");
    Ok(reply(json!({ "id": user })))
}

pub async fn reset_password(
    State(ctx): State<Ctx>,
    Path(id): Path<String>,
    Json(reset): Json<Reset>,
) -> ApiResult<Response> {
    let user = UserId(id);
    let store = &ctx.backend.store;
    store.user(&user)?.ok_or_else(missing)?;
    store.set_password(&user, &hashed(&reset.password)?, settings::now())?;
    store.close_sessions_of(&user)?;
    tracing::info!(target: "audit", user = %user, "auth.password_reset_by_admin");
    Ok(done())
}

fn fingerprint(login: &str, password: &str) -> String {
    platform::crypto::hex(&platform::crypto::sha256(format!("{login}\n{password}").as_bytes()))
}

pub fn bootstrap(store: &Store) -> Result<(), String> {
    let file = settings::admin_file();
    if file.is_empty() || settings::mode() != settings::Mode::Shared {
        return Ok(());
    }
    let text = std::fs::read_to_string(file).map_err(|error| format!("{file}: {error}"))?;
    let given: Bootstrap = serde_json::from_str(&text).map_err(|error| format!("{file}: {error}"))?;
    let login = normalized_login(&given.login).ok_or_else(|| format!("{file}: the login is not valid"))?;
    if given.password.chars().count() < SHORTEST {
        return Err(format!("{file}: the password needs at least {SHORTEST} characters"));
    }
    let now = settings::now();
    let wanted =
        Principal { user: UserId(format!("u-{}", uuid::Uuid::new_v4().simple())), email: login, name: String::new() };
    let person = store.ensure_user(&wanted, now).map_err(|error| error.to_string())?;
    let print = fingerprint(&person.email, &given.password);
    let applied = store.preference(BOOTSTRAPPED).map_err(|error| error.to_string())?;
    if applied.as_ref().and_then(|value| value.as_str()) == Some(print.as_str()) {
        return Ok(());
    }
    let hash = platform::password::hash(&given.password)?;
    store.set_password(&person.user, &hash, now).map_err(|error| error.to_string())?;
    store.set_preference(BOOTSTRAPPED, &json!(print)).map_err(|error| error.to_string())?;
    tracing::info!(target: "audit", user = %person.user, "auth.admin_bootstrapped");
    Ok(())
}
