use domain::effect::{Call, Effect, EffectOutcome, EffectRequest};
use serde_json::{Value, json};

use crate::ledger::Ledger;
use crate::shared::now_ms;

const NAMES: [&str; 6] = ["title", "name", "issue", "key", "change", "project"];

pub fn calls_block(ledger: &Ledger, step: &str) -> Option<Value> {
    let mine: Vec<&EffectRequest> = ledger.requested.values().filter(|request| request.key.step == step).collect();
    let attempt = mine.iter().map(|request| request.key.attempt).max()?;
    let calls: Vec<Value> = mine
        .into_iter()
        .filter(|request| request.key.attempt == attempt)
        .filter_map(|request| match &request.effect {
            Effect::ConnectorCall(call) => Some(row(ledger, request.key.seq, call)),
            _ => None,
        })
        .collect();
    (!calls.is_empty()).then(|| json!({ "type": "calls", "calls": calls }))
}

fn row(ledger: &Ledger, seq: u64, call: &Call) -> Value {
    let started = ledger.requested_at.get(&seq).copied().unwrap_or_else(now_ms);
    let resolved = ledger.resolved.get(&seq);
    let ended = resolved.map_or_else(now_ms, |resolution| resolution.at);
    let (state, output) = match resolved.map(|resolution| &resolution.outcome) {
        None => ("running", None),
        Some(EffectOutcome::Ok { value }) => ("ok", Some(value.to_string())),
        Some(EffectOutcome::Failed { message, .. }) => ("error", Some(message.clone())),
        Some(EffectOutcome::TimedOut) => ("error", Some("tempo esgotado".to_owned())),
        Some(EffectOutcome::Cancelled) => ("error", Some("cancelado".to_owned())),
    };
    let mut found = json!({
        "id": seq.to_string(),
        "kind": "call",
        "title": title(call),
        "tag": call.capability,
        "state": state,
        "input": call.args,
        "durationMs": (ended - started).max(0),
    });
    if let Some(output) = output {
        found["output"] = Value::String(output);
    }
    found
}

fn title(call: &Call) -> String {
    let action = call.action.strip_prefix(&format!("{}.", call.capability)).unwrap_or(&call.action);
    let named = NAMES.iter().find_map(|name| call.args[*name].as_str().filter(|text| !text.is_empty()));
    match named {
        Some(name) => format!("{action} · {name}"),
        None => action.to_owned(),
    }
}
