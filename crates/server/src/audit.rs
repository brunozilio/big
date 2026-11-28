use std::collections::HashMap;
use std::sync::Arc;

use axum::extract::{Query, State};
use axum::http::header::{CONTENT_DISPOSITION, CONTENT_TYPE};
use axum::response::{IntoResponse, Response};
use serde_json::json;
use store::{AuditFilter, AuditPage, Store};

use crate::error::{ApiError, ApiResult, json as reply};
use crate::state::Ctx;

const PAGE: usize = 50;
const MOST_PAGE: usize = 500;
const EXPORT_PAGE: usize = 1000;

type Asked = Query<HashMap<String, String>>;

fn text(query: &HashMap<String, String>, key: &str) -> Option<String> {
    query.get(key).map(|value| value.trim().to_owned()).filter(|value| !value.is_empty())
}

fn number<T: std::str::FromStr>(query: &HashMap<String, String>, key: &str) -> Option<T> {
    text(query, key).and_then(|value| value.parse().ok())
}

pub fn filter_of(query: &HashMap<String, String>) -> AuditFilter {
    AuditFilter {
        actor: text(query, "agent"),
        subject: text(query, "person"),
        action: text(query, "action"),
        from: number(query, "from"),
        to: number(query, "to"),
    }
}

async fn blocking<T: Send + 'static>(
    ctx: &Ctx,
    work: impl FnOnce(&Store) -> store::Result<T> + Send + 'static,
) -> ApiResult<T> {
    let store = Arc::clone(&ctx.backend.store);
    Ok(tokio::task::spawn_blocking(move || work(&store)).await.map_err(ApiError::failed)??)
}

pub async fn list(State(ctx): State<Ctx>, Query(query): Asked) -> ApiResult<Response> {
    let filter = filter_of(&query);
    let limit = number(&query, "limit").unwrap_or(PAGE).clamp(1, MOST_PAGE);
    let page = AuditPage::Before(number(&query, "before"));
    let records = blocking(&ctx, move |store| store.audit(&filter, page, limit)).await?;
    let next = (records.len() == limit).then(|| records.last().map(|record| record.seq)).flatten();
    Ok(reply(json!({ "records": records, "next": next })))
}

fn exported(store: &Store, filter: &AuditFilter) -> store::Result<String> {
    let mut out = String::new();
    let mut after = 0;
    loop {
        let records = store.audit(filter, AuditPage::After(after), EXPORT_PAGE)?;
        for record in &records {
            out.push_str(&serde_json::to_string(record)?);
            out.push('\n');
        }
        match records.last() {
            Some(last) if records.len() == EXPORT_PAGE => after = last.seq,
            _ => return Ok(out),
        }
    }
}

pub async fn export(State(ctx): State<Ctx>, Query(query): Asked) -> ApiResult<Response> {
    let filter = filter_of(&query);
    let lines = blocking(&ctx, move |store| exported(store, &filter)).await?;
    let name = format!("attachment; filename=\"audit-{}.jsonl\"", jiff::Timestamp::now().strftime("%Y%m%d-%H%M%S"));
    Ok(([(CONTENT_TYPE, "application/x-ndjson".to_owned()), (CONTENT_DISPOSITION, name)], lines).into_response())
}

pub async fn facets(State(ctx): State<Ctx>) -> ApiResult<Response> {
    Ok(reply(blocking(&ctx, |store| store.audit_facets()).await?))
}

pub async fn verify(State(ctx): State<Ctx>) -> ApiResult<Response> {
    Ok(reply(blocking(&ctx, |store| store.verify_audit()).await?))
}
