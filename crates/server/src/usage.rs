use std::collections::HashMap;
use std::sync::Arc;

use app::ProviderSummary;
use axum::extract::{Query, State};
use axum::response::Response;
use domain::EngineEvent;
use domain::models::usage_report::{Tally, UNKNOWN, UsageReport, UsageTally};
use domain::trace::Record;
use jiff::civil::Date;
use jiff::tz::TimeZone;
use jiff::{Timestamp, ToSpan, Zoned};
use serde_json::{Value, json};

use crate::error::{ApiError, ApiResult, json as reply};
use crate::state::{Ctx, Shared};

const WINDOWS: [i64; 3] = [7, 30, 90];
const DEFAULT_DAYS: i64 = 30;
const UNRECORDED: &str = "Sem registro";

pub fn days_of(query: &HashMap<String, String>) -> i64 {
    query
        .get("days")
        .and_then(|days| days.trim().parse::<i64>().ok())
        .filter(|days| WINDOWS.contains(days))
        .unwrap_or(DEFAULT_DAYS)
}

pub fn window(today: Date, days: i64) -> Vec<String> {
    (0..days).rev().filter_map(|back| today.checked_sub(back.days()).ok()).map(|date| date.to_string()).collect()
}

fn day_of(at_ms: u64, zone: &TimeZone) -> Option<String> {
    let millis = i64::try_from(at_ms).ok()?;
    Some(Timestamp::from_millisecond(millis).ok()?.to_zoned(zone.clone()).date().to_string())
}

fn first_second(today: &Zoned, days: i64) -> u64 {
    let first = today.date().checked_sub((days - 1).days()).unwrap_or(today.date());
    let start = first.to_zoned(today.time_zone().clone()).map_or(0, |start| start.timestamp().as_second());
    u64::try_from(start).unwrap_or(0)
}

fn gather(shared: &Shared, days: i64) -> ApiResult<Value> {
    let now = Zoned::now();
    let zone = now.time_zone().clone();
    let window = window(now.date(), days);
    let start = first_second(&now, days);
    let client = &shared.backend.client;
    let providers = client.providers().unwrap_or_default();
    let known: Vec<&str> = providers.iter().map(|summary| summary.provider.id()).collect();
    let mut tally = UsageTally::new(&window, &known);
    let sessions = client.sessions().map_err(ApiError::failed)?;
    for listed in sessions.iter().filter(|listed| listed.updated == 0 || listed.updated >= start) {
        let mut records = match client.history(&listed.session, &listed.cwd) {
            Ok(records) => records,
            Err(error) => {
                tracing::warn!(%error, session = %listed.session.0, "the session's history could not be read for usage");
                continue;
            }
        };
        for record in &mut records {
            if let Record::Event { at_ms, event } = record
                && let Some(day) = day_of(*at_ms, &zone)
            {
                priced(event);
                tally.add(&listed.session.0, &day, event);
            }
        }
    }
    Ok(listing(days, &window, &tally.report(), &providers))
}

pub fn priced(event: &mut EngineEvent) {
    match event {
        EngineEvent::Usage(usage) if usage.estimated_cost_usd.is_none() => {
            let provider = match usage.provider.trim() {
                "" => usage.model.split_once(':').map_or("", |(provider, _)| provider).to_owned(),
                served => served.to_owned(),
            };
            usage.estimated_cost_usd = app::estimated_cost(&provider, usage);
        }
        EngineEvent::AgentEvent { event, .. } => priced(event),
        _ => {}
    }
}

fn named(id: &str, providers: &[ProviderSummary]) -> (Value, Value) {
    if id == UNKNOWN {
        return (json!(UNRECORDED), json!(UNKNOWN));
    }
    match providers.iter().find(|summary| summary.provider.id() == id) {
        Some(summary) => {
            let described = crate::ai::provider_json(summary);
            (described["name"].clone(), described["logo"].clone())
        }
        None => (json!(id), json!(id)),
    }
}

fn counted(mut fields: Value, tally: &Tally) -> Value {
    fields["costUsd"] = json!(tally.cost_usd);
    fields["tokens"] = json!(tally.tokens);
    fields["calls"] = json!(tally.calls);
    fields
}

pub fn listing(days: i64, window: &[String], report: &UsageReport, providers: &[ProviderSummary]) -> Value {
    let totals = &report.totals;
    let daily: Vec<Value> = report
        .daily
        .iter()
        .map(|day| {
            counted(json!({ "date": day.date, "byProvider": day.by_provider, "byModel": day.by_model }), &day.tally)
        })
        .collect();
    let by_provider: Vec<Value> = report
        .by_provider
        .iter()
        .map(|ranked| {
            let (name, logo) = named(&ranked.id, providers);
            counted(json!({ "id": ranked.id, "name": name, "logo": logo }), &ranked.tally)
        })
        .collect();
    let by_model: Vec<Value> = report
        .by_model
        .iter()
        .map(|ranked| counted(json!({ "id": ranked.id, "name": ranked.id }), &ranked.tally))
        .collect();
    let by_account: Vec<Value> = report
        .by_account
        .iter()
        .map(|ranked| {
            let id = format!("{}:{}", ranked.provider, ranked.account);
            counted(json!({ "id": id, "provider": ranked.provider, "account": ranked.account }), &ranked.tally)
        })
        .collect();
    json!({
        "days": days, "from": window.first(), "to": window.last(),
        "totals": {
            "costUsd": totals.cost_usd, "input": totals.input, "output": totals.output,
            "cacheRead": totals.cache_read, "cacheWrite": totals.cache_write,
            "calls": totals.calls, "sessions": totals.sessions,
        },
        "daily": daily, "byProvider": by_provider, "byModel": by_model, "byAccount": by_account,
    })
}

pub async fn report(State(ctx): State<Ctx>, Query(query): Query<HashMap<String, String>>) -> ApiResult<Response> {
    let shared = Arc::clone(&ctx);
    let days = days_of(&query);
    let listed = tokio::task::spawn_blocking(move || gather(&shared, days)).await.map_err(ApiError::failed)??;
    Ok(reply(listed))
}

#[cfg(test)]
#[path = "usage_tests.rs"]
mod tests;
