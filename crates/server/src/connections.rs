use std::collections::HashMap;
use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, header};
use axum::response::{Html, IntoResponse, Redirect, Response};
use connectors::{AuthSpec, ProviderDescriptor, SettingsSink, TokenField};
use domain::Language;
use domain::connection::ConnectionId;
use serde_json::{Value, json};
use store::{ConnectionRecord, Store};
use sync::ConnectionState;

use crate::error::{ApiError, ApiResult, done, json as reply, said};
use crate::marks::logo;
use crate::state::{Ctx, Shared};

fn group(descriptor: &ProviderDescriptor) -> &'static str {
    let first = descriptor.capabilities.first().map(|capability| capability.as_str()).unwrap_or_default();
    match first {
        "tracker" => "tracker",
        "cloud" => "cloud",
        "mcp" => "mcp",
        "tests" => "tests",
        "security" => "security",
        "changes" => "changes",
        _ => "code",
    }
}

fn fields_of(fields: &[TokenField]) -> Value {
    let fields: Vec<Value> = fields
        .iter()
        .map(|field| {
            let help = (!field.help.is_empty()).then_some(&field.help);
            let value = (!field.value.is_empty()).then_some(&field.value);
            let placeholder = (!field.placeholder.is_empty()).then_some(&field.placeholder);
            json!({
                "name": field.name, "label": field.label, "kind": "text", "mono": true,
                "secret": field.secret, "help": help, "value": value, "placeholder": placeholder,
            })
        })
        .collect();
    Value::Array(fields)
}

fn sign_in_of(auth: &AuthSpec, language: Language) -> (&'static str, Value) {
    match auth {
        AuthSpec::Loopback(sign_in) => ("oauth", fields_of(&sign_in.0.fields(language))),
        AuthSpec::Device(_) => ("device", json!([])),
        AuthSpec::Steps(sign_in) => ("device", fields_of(&sign_in.0.fields(language))),
        AuthSpec::Token(spec) => ("token", fields_of(&spec.fields)),
    }
}

fn said_with(shared: &Shared, english: &str, spanish: &str, portuguese: &str, name: &str) -> String {
    shared.language().pick(english, spanish, portuguese).replace("{}", name)
}

fn or(text: &str, fallback: String) -> String {
    if text.is_empty() { fallback } else { text.to_owned() }
}

fn provider_json(shared: &Shared, descriptor: &ProviderDescriptor) -> Value {
    let connectors = &shared.backend.connectors;
    let look = connectors.presentation(&descriptor.key, shared.language()).unwrap_or_default();
    let (sign_in, fields) = sign_in_of(&descriptor.auth, shared.language());
    let alternatives: Vec<Value> = connectors
        .alternatives(&descriptor.key, shared.language())
        .unwrap_or_default()
        .iter()
        .map(|alternative| {
            let (sign_in, fields) = sign_in_of(&alternative.auth, shared.language());
            json!({
                "key": alternative.key, "label": alternative.label, "button": alternative.button,
                "site": alternative.site, "signIn": sign_in, "fields": fields,
            })
        })
        .collect();
    json!({
        "key": descriptor.key.as_str(), "name": descriptor.name,
        "group": or(&look.group, group(descriptor).to_owned()),
        "description": or(&look.description, said_with(shared, "Your {} account", "Tu cuenta de {}", "Sua conta {}", &descriptor.name)),
        "button": or(&look.button, said_with(shared, "Sign in with {}", "Iniciar sesión con {}", "Entrar com {}", &descriptor.name)),
        "site": or(&look.site, said_with(shared, "from {}", "de {}", "do {}", &descriptor.name)),
        "logo": logo(&descriptor.logo), "signIn": sign_in, "fields": fields, "multiple": true,
        "alternatives": alternatives,
    })
}

pub fn record_json(record: &ConnectionRecord, status: &str) -> Value {
    json!({
        "id": record.id, "provider": record.provider, "label": record.label,
        "account": record.account.clone().unwrap_or_default(), "capabilities": record.capabilities, "status": status,
    })
}

fn status(shared: &Shared, record: &ConnectionRecord) -> &'static str {
    let health = shared.backend.orchestrator.health();
    let state = health.map(|health| health.state(&ConnectionId::from(record.id.as_str())));
    match state {
        Some(ConnectionState::NeedsReconnect { .. }) => "error",
        _ => "ok",
    }
}

pub fn listing(shared: &Shared) -> ApiResult<Value> {
    let descriptors: Vec<ProviderDescriptor> = shared.backend.connectors.descriptors().into_iter().cloned().collect();
    let ai = crate::ai::subscriptions(shared);
    let connectors = &shared.backend.connectors;
    let mut providers: Vec<Value> = descriptors
        .iter()
        .filter(|descriptor| !connectors.presentation(&descriptor.key, shared.language()).is_ok_and(|look| look.hidden))
        .map(|descriptor| provider_json(shared, descriptor))
        .collect();
    providers.extend(ai.iter().map(|summary| crate::ai::provider_json(summary, shared.language())));
    let records = shared.backend.store.connections()?;
    let mut connections: Vec<Value> =
        records.iter().map(|record| record_json(record, status(shared, record))).collect();
    connections.extend(ai.iter().flat_map(crate::ai::connections_of));
    Ok(json!({ "providers": providers, "connections": connections }))
}

pub fn announce(shared: &Shared) {
    if let Ok(listed) = listing(shared) {
        shared.live.connections(listed);
    }
}

async fn blocking<T: Send + 'static>(work: impl FnOnce() -> ApiResult<T> + Send + 'static) -> ApiResult<T> {
    tokio::task::spawn_blocking(work).await.map_err(ApiError::failed)?
}

pub async fn list(State(ctx): State<Ctx>) -> ApiResult<Response> {
    let shared = Arc::clone(&ctx);
    Ok(reply(blocking(move || listing(&shared)).await?))
}

fn is_connector(shared: &Shared, provider: &str) -> bool {
    shared.backend.connectors.descriptors().iter().any(|descriptor| descriptor.key.as_str() == provider)
}

pub async fn start(
    State(ctx): State<Ctx>,
    Path(provider): Path<String>,
    headers: HeaderMap,
    body: Option<Json<Value>>,
) -> ApiResult<Response> {
    let body = body.map(|Json(body)| body).unwrap_or(Value::Null);
    let fields = body["fields"].clone();
    if is_connector(&ctx, &provider) {
        return Ok(reply(crate::sign_in::start(&ctx, &provider, &fields, body["method"].as_str()).await?));
    }
    let shared = Arc::clone(&ctx);
    let host = headers.get(header::HOST).and_then(|host| host.to_str().ok()).map(str::to_owned);
    Ok(reply(blocking(move || crate::ai::start(&shared, &provider, host.as_deref())).await?))
}

pub async fn poll(State(ctx): State<Ctx>, Json(body): Json<Value>) -> ApiResult<Response> {
    let flow = body["flow"].as_str().ok_or_else(|| {
        ApiError::bad(said("Say which sign-in.", "Indica qué inicio de sesión.", "Diga qual entrada."))
    })?;
    Ok(reply(crate::sign_in::poll(&ctx, flow, body["choice"].as_str()).await?))
}

pub async fn callback(State(ctx): State<Ctx>, Query(query): Query<HashMap<String, String>>) -> Response {
    answered(&ctx, &query, "").await
}

pub async fn answered(ctx: &Shared, query: &HashMap<String, String>, base: &str) -> Response {
    let told = query.get("error_description").or(query.get("error")).cloned().unwrap_or_default();
    let Some(state) = query.get("state") else {
        let failed = ctx.language().pick("The sign-in failed.", "El inicio de sesión falló.", "A entrada falhou.");
        return Html(format!("<!doctype html><meta charset=\"utf-8\"><p>{failed} {told}</p>")).into_response();
    };
    let back = match query.get("code") {
        Some(code) => crate::sign_in::callback(ctx, state, code).await,
        None => crate::sign_in::refused(ctx, state, &told),
    };
    match back {
        Ok(back) => Redirect::to(&format!("{base}{back}")).into_response(),
        Err(error) => error.into_response(),
    }
}

pub async fn remove(State(ctx): State<Ctx>, Path(id): Path<String>) -> ApiResult<Response> {
    if id.starts_with(crate::ai::PREFIX) {
        let shared = Arc::clone(&ctx);
        blocking(move || crate::ai::disconnect(&shared, &id)).await?;
    } else {
        if !ctx.backend.store.delete_connection(&id)? {
            return Err(ApiError::missing(said(
                "There is no such connection.",
                "No existe esa conexión.",
                "Não há essa conexão.",
            )));
        }
        let _ = ctx.backend.connectors.keeper().store().remove(&ConnectionId::from(id.as_str())).await;
    }
    let shared = Arc::clone(&ctx);
    let _ = tokio::task::spawn_blocking(move || announce(&shared)).await;
    Ok(done())
}

pub async fn call(State(ctx): State<Ctx>, Path(id): Path<String>, Json(body): Json<Value>) -> ApiResult<Response> {
    let record = ctx.backend.store.connection(&id)?.ok_or_else(|| {
        ApiError::missing(said("There is no such connection.", "No existe esa conexión.", "Não há essa conexão."))
    })?;
    let action = body["action"]
        .as_str()
        .ok_or_else(|| ApiError::bad(said("Say the action.", "Indica la acción.", "Diga a ação.")))?;
    let result = crate::calls::run(&ctx, &record, action, &body["args"]).await;
    if let Err(error) = &result
        && error.status == axum::http::StatusCode::BAD_GATEWAY
    {
        tracing::warn!(connection = %id, %action, message = %error.message, "a read for the page failed");
    }
    Ok(reply(result?))
}

#[derive(Debug)]
pub struct StoredSettings(pub Arc<Store>);

impl SettingsSink for StoredSettings {
    fn learned(&self, connection: &ConnectionId, settings: &Value) {
        let Ok(Some(mut record)) = self.0.connection(connection.as_str()) else { return };
        record.settings = connectors::hub::filled(&record.settings, settings);
        if let Err(error) = self.0.save_connection(&record) {
            tracing::warn!(%error, connection = %connection, "the settings learned for a connection could not be kept");
        }
    }
}
