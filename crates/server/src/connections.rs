use std::collections::HashMap;
use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::response::{Html, IntoResponse, Response};
use connectors::{AuthSpec, ProviderDescriptor};
use domain::connection::ConnectionId;
use serde_json::{Value, json};
use store::ConnectionRecord;
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

fn provider_json(descriptor: &ProviderDescriptor) -> Value {
    let (sign_in, fields) = match &descriptor.auth {
        AuthSpec::Redirect(_) => ("oauth", json!([])),
        AuthSpec::Device(_) => ("device", json!([])),
        AuthSpec::Sso(spec) => {
            let field = json!({ "name": spec.start_field, "label": "Start URL", "kind": "text", "mono": true });
            ("sso", json!([field]))
        }
    };
    json!({
        "key": descriptor.key.as_str(), "name": descriptor.name, "group": group(descriptor),
        "description": format!("Sua conta {}", descriptor.name), "logo": logo(&descriptor.logo),
        "signIn": sign_in, "fields": fields, "multiple": true,
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
    let mut providers: Vec<Value> = descriptors.iter().map(provider_json).collect();
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
    let fields = body.map(|Json(body)| body["fields"].clone()).unwrap_or(Value::Null);
    if is_connector(&ctx, &provider) {
        return Ok(reply(crate::sign_in::start(&ctx, &provider, &fields).await?));
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
