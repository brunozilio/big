use axum::Json;
use axum::extract::State;
use axum::response::{IntoResponse, Response};
use domain::Language;
use domain::principal::{Principal, UserId, domain_of, normalized_email};
use serde::Deserialize;
use store::CodeCheck;

use super::mail::{self, Letter};
use super::settings::{self, Method};
use super::session;
use crate::error::{ApiError, ApiResult, done, json, said};
use crate::state::{Ctx, Shared};

#[derive(Debug, Deserialize)]
pub struct Asked {
    email: String,
}

#[derive(Debug, Deserialize)]
pub struct Answered {
    email: String,
    code: String,
}

fn bad_email() -> ApiError {
    ApiError::bad(said("Type a valid email.", "Escribe un correo válido.", "Digite um e-mail válido."))
}

fn wrong_code() -> ApiError {
    ApiError::bad(said(
        "That code is wrong or has expired.",
        "Ese código es incorrecto o venció.",
        "Esse código está errado ou expirou.",
    ))
}

pub fn closed() -> ApiError {
    ApiError::bad(said(
        "This way of signing in is off.",
        "Esta forma de entrar está desactivada.",
        "Essa forma de entrar está desligada.",
    ))
}

pub fn admitted(shared: &Shared, email: &str) -> ApiResult<bool> {
    if shared.backend.store.user_by_email(email)?.is_some() {
        return Ok(true);
    }
    Ok(settings::allowed_domains().iter().any(|domain| domain == domain_of(email)))
}

fn code() -> ApiResult<String> {
    let bytes = platform::crypto::random(4).map_err(ApiError::failed)?;
    let word: [u8; 4] = bytes.try_into().map_err(|_| ApiError::failed("random"))?;
    let number = u32::from_be_bytes(word) % 1_000_000;
    Ok(format!("{number:06}"))
}

fn letter(language: Language, to: &str, code: &str) -> Letter {
    let name = brand::NAME;
    let minutes = settings::code_seconds() / 60;
    let subject = language.pick(
        format!("{name} sign-in code: {code}"),
        format!("Código para entrar en {name}: {code}"),
        format!("Código para entrar no {name}: {code}"),
    );
    let body = language.pick(
        format!("Your code is {code}. It lasts {minutes} minutes."),
        format!("Tu código es {code}. Vale {minutes} minutos."),
        format!("Seu código é {code}. Vale {minutes} minutos."),
    );
    Letter { to: to.to_owned(), subject, body }
}

pub async fn start(State(ctx): State<Ctx>, Json(asked): Json<Asked>) -> ApiResult<Response> {
    if !settings::offers(Method::Email) {
        return Err(closed());
    }
    let email = normalized_email(&asked.email).ok_or_else(bad_email)?;
    if admitted(&ctx, &email)? {
        let code = code()?;
        let expires = settings::now() + settings::code_seconds();
        ctx.backend.store.put_code(&email, &session::hashed(&code), expires)?;
        if let Err(error) = mail::send(&letter(ctx.language(), &email, &code)).await {
            tracing::error!(%error, "the sign-in mail could not be sent");
            return Err(ApiError::upstream(error));
        }
    } else {
        tracing::warn!(target: "audit", domain = domain_of(&email), "auth.email_refused");
    }
    Ok(done())
}

pub async fn verify(State(ctx): State<Ctx>, Json(answered): Json<Answered>) -> ApiResult<Response> {
    if !settings::offers(Method::Email) {
        return Err(closed());
    }
    let email = normalized_email(&answered.email).ok_or_else(bad_email)?;
    let code = answered.code.trim();
    let check =
        ctx.backend.store.check_code(&email, &session::hashed(code), settings::now(), settings::code_attempts())?;
    if check != CodeCheck::Matched {
        return Err(wrong_code());
    }
    let wanted = Principal { user: UserId(format!("u-{}", uuid::Uuid::new_v4().simple())), email, name: String::new() };
    let person = ctx.backend.store.ensure_user(&wanted, settings::now())?;
    let headers = session::signed_in(&ctx, &person)?;
    Ok((headers, json(&person)).into_response())
}
