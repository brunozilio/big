use std::collections::HashMap;
use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::response::{Html, IntoResponse, Response};
use connectors::{AuthSpec, ProviderDescriptor, SettingsSink};
use domain::connection::ConnectionId;
use serde_json::{Value, json};
use store::{ConnectionRecord, Store};
use sync::ConnectionState;

use crate::error::{ApiError, ApiResult, done, json as reply};
use crate::marks::logo;
use crate::state::{Ctx, Shared};

fn group(descriptor: &ProviderDescriptor) -> &'static str {
    let first = descriptor.capabilities.first().map(|capability| capability.as_str()).unwrap_or_default();
    match first {
        "tracker" => "Tarefas",
        "cloud" => "Nuvem",
        _ => "Código",
    }
}

fn sign_in_of(auth: &AuthSpec) -> (&'static str, Value) {
    match auth {
        AuthSpec::Redirect(_) | AuthSpec::Loopback(_) => ("oauth", json!([])),
        AuthSpec::Device(_) => ("device", json!([])),
        AuthSpec::Sso(spec) => {
            let start = json!({
                "name": spec.start_field, "label": "Start URL do IAM Identity Center", "kind": "text", "mono": true,
                "placeholder": "https://empresa.awsapps.com/start",
            });
            let region =
                json!({ "name": "region", "label": "Região", "kind": "text", "mono": true, "value": spec.region });
            ("sso", json!([start, region]))
        }
    }
}

fn or(text: &str, fallback: String) -> String {
    if text.is_empty() { fallback } else { text.to_owned() }
}

fn provider_json(shared: &Shared, descriptor: &ProviderDescriptor) -> Value {
    let connectors = &shared.backend.connectors;
    let look = connectors.presentation(&descriptor.key).cloned().unwrap_or_default();
    let (sign_in, fields) = sign_in_of(&descriptor.auth);
    let alternative = connectors.alternative(&descriptor.key).ok().flatten().map(|alternative| {
        let (sign_in, fields) = sign_in_of(&alternative.auth);
        json!({
            "key": alternative.key, "label": alternative.label, "button": alternative.button, "site": alternative.site,
            "signIn": sign_in, "fields": fields,
        })
    });
    json!({
        "key": descriptor.key.as_str(), "name": descriptor.name,
        "group": or(&look.group, group(descriptor).to_owned()),
        "description": or(&look.description, format!("Sua conta {}", descriptor.name)),
        "button": or(&look.button, format!("Entrar com {}", descriptor.name)),
        "site": or(&look.site, format!("do {}", descriptor.name)),
        "logo": logo(&descriptor.logo), "signIn": sign_in, "fields": fields, "multiple": true,
        "alternative": alternative,
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
        Some(ConnectionState::NeedsReconnect { .. } | ConnectionState::Failing { .. }) => "error",
        _ => "ok",
    }
}

pub fn listing(shared: &Shared) -> ApiResult<Value> {
    let descriptors: Vec<ProviderDescriptor> = shared.backend.connectors.descriptors().into_iter().cloned().collect();
    let ai = crate::ai::subscriptions(shared);
    let mut providers: Vec<Value> = descriptors.iter().map(|descriptor| provider_json(shared, descriptor)).collect();
    providers.extend(ai.iter().map(crate::ai::provider_json));
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
    body: Option<Json<Value>>,
) -> ApiResult<Response> {
    let body = body.map(|Json(body)| body).unwrap_or(Value::Null);
    let fields = body["fields"].clone();
    if is_connector(&ctx, &provider) {
        return Ok(reply(crate::sign_in::start(&ctx, &provider, &fields, body["method"].as_str()).await?));
    }
    let shared = Arc::clone(&ctx);
    Ok(reply(blocking(move || crate::ai::start(&shared, &provider)).await?))
}

pub async fn poll(State(ctx): State<Ctx>, Json(body): Json<Value>) -> ApiResult<Response> {
    let flow = body["flow"].as_str().ok_or_else(|| ApiError::bad("Diga qual entrada."))?;
    Ok(reply(crate::sign_in::poll(&ctx, flow, body["choice"].as_str()).await?))
}

const SIGNED_IN: &str = "<!doctype html><meta charset=\"utf-8\"><title>Conectado</title>\
<body style=\"font-family:system-ui;margin:4rem auto;max-width:28rem;text-align:center\">\
<h1>Pronto</h1><p>Pode fechar esta aba e voltar.</p></body>";

pub async fn callback(State(ctx): State<Ctx>, Query(query): Query<HashMap<String, String>>) -> Response {
    let (state, code) = (query.get("state"), query.get("code"));
    let (Some(state), Some(code)) = (state, code) else {
        let told = query.get("error_description").or(query.get("error")).cloned().unwrap_or_default();
        return Html(format!("<!doctype html><meta charset=\"utf-8\"><p>A entrada falhou. {told}</p>")).into_response();
    };
    match crate::sign_in::callback(&ctx, state, code).await {
        Ok(()) => Html(SIGNED_IN).into_response(),
        Err(error) => error.into_response(),
    }
}

pub async fn remove(State(ctx): State<Ctx>, Path(id): Path<String>) -> ApiResult<Response> {
    if id.starts_with(crate::ai::PREFIX) {
        let shared = Arc::clone(&ctx);
        blocking(move || crate::ai::disconnect(&shared, &id)).await?;
    } else {
        if !ctx.backend.store.delete_connection(&id)? {
            return Err(ApiError::missing("Não há essa conexão."));
        }
        let _ = ctx.backend.connectors.keeper().store().remove(&ConnectionId::from(id.as_str())).await;
    }
    let shared = Arc::clone(&ctx);
    let _ = tokio::task::spawn_blocking(move || announce(&shared)).await;
    Ok(done())
}

pub async fn call(State(ctx): State<Ctx>, Path(id): Path<String>, Json(body): Json<Value>) -> ApiResult<Response> {
    let record = ctx.backend.store.connection(&id)?.ok_or_else(|| ApiError::missing("Não há essa conexão."))?;
    let action = body["action"].as_str().ok_or_else(|| ApiError::bad("Diga a ação."))?;
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
