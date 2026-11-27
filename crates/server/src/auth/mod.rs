mod email;
mod gate;
mod mail;
mod passkey;
mod password;
pub mod session;
pub mod settings;
mod sso;

use axum::Router;
use axum::extract::State;
use axum::http::HeaderMap;
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, post, put};
use serde_json::json;

pub use gate::gate;
pub use password::{add_person, reset_password};

use crate::error::{ApiResult, json as reply};
use crate::state::Ctx;

pub fn routes() -> Router<Ctx> {
    Router::new()
        .route("/api/auth/methods", get(methods))
        .route("/api/auth/me", get(me))
        .route("/api/auth/logout", post(logout))
        .route("/api/auth/password", post(password::sign_in))
        .route("/api/auth/setup", post(password::set_up))
        .route("/api/account/password", put(password::change))
        .route("/api/auth/email", post(email::start))
        .route("/api/auth/email/verify", post(email::verify))
        .route("/api/auth/sso/start", get(sso::start))
        .route(sso::CALLBACK, get(sso::callback))
        .route("/api/auth/passkey/options", post(passkey::sign_in_options))
        .route("/api/auth/passkey", post(passkey::sign_in))
        .route("/api/account/passkeys", get(passkey::list).post(passkey::enroll))
        .route("/api/account/passkeys/options", post(passkey::enroll_options))
        .route("/api/account/passkeys/{id}", delete(passkey::remove))
}

pub fn person_of(shared: &crate::state::Shared, headers: &HeaderMap) -> Option<domain::principal::Principal> {
    session::person(shared, headers).ok().flatten()
}

fn mode_name() -> &'static str {
    match settings::mode() {
        settings::Mode::Local => "local",
        settings::Mode::Shared => "shared",
    }
}

async fn methods(State(ctx): State<Ctx>) -> ApiResult<Response> {
    let offered: Vec<&str> =
        settings::methods().into_iter().filter(|method| settings::offers(*method)).map(settings::Method::key).collect();
    let label = brand::auth::SSO_LABEL.trim();
    let setup = password::setup_open(&ctx.backend.store)?;
    Ok(reply(json!({ "mode": mode_name(), "methods": offered, "ssoLabel": label, "setup": setup })))
}

async fn me(State(ctx): State<Ctx>, headers: HeaderMap) -> ApiResult<Response> {
    match session::person(&ctx, &headers)? {
        Some(person) => {
            let admin = crate::scope::is_admin(&person);
            Ok(reply(json!({ "mode": mode_name(), "user": person, "admin": admin })))
        }
        None => Err(gate::signed_out_error()),
    }
}

async fn logout(State(ctx): State<Ctx>, headers: HeaderMap) -> ApiResult<Response> {
    let cleared = session::signed_out(&ctx, &headers)?;
    Ok((cleared, crate::error::done()).into_response())
}
