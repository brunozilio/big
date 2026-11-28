use std::collections::BTreeSet;
use std::path::Path;

use serde_json::{Value, json};

pub const LEDGER: &str = "content/published.json";
pub const RETIRED: &str = "content/retired.json";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Successor {
    By(String),
    Removed { reason: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Retired {
    pub from: String,
    pub to: Successor,
}

pub struct Ledgers<'a> {
    pub current: &'a BTreeSet<String>,
    pub ledger: &'a BTreeSet<String>,
    pub before: &'a BTreeSet<String>,
    pub retired: &'a [Retired],
}

pub fn check(given: &Ledgers) -> Vec<String> {
    let retired: BTreeSet<&str> = given.retired.iter().map(|retired| retired.from.as_str()).collect();
    let gone = |id: &&String| !given.current.contains(*id) && !retired.contains(id.as_str());
    let mut found: Vec<String> = given
        .current
        .difference(given.ledger)
        .map(|id| format!("published-id {id}: new id, add it with `xtask conformance --publish`"))
        .collect();
    found.extend(given.ledger.iter().filter(gone).map(|id| {
        format!(
            "published-id {id}: removed without retirement; keep it, or retire it in {RETIRED} (ids) after migrating"
        )
    }));
    found.extend(given.before.difference(given.ledger).map(|id| {
        format!("published-id {id}: left {LEDGER}; a published id is retired, never deleted from the ledger")
    }));
    for entry in given.retired {
        if given.current.contains(&entry.from) {
            found.push(format!("published-id {}: retired, so it cannot be published again", entry.from));
        }
        if let Successor::By(to) = &entry.to
            && !given.current.contains(to)
        {
            found.push(format!("published-id {}: retired in favor of {to}, which is not published", entry.from));
        }
    }
    found
}

pub fn ids_of(text: &str) -> Result<BTreeSet<String>, String> {
    let value: Value = serde_json::from_str(text).map_err(|error| format!("{LEDGER} does not read: {error}"))?;
    let listed = value["ids"].as_array().ok_or_else(|| format!("{LEDGER} has no ids list"))?;
    listed
        .iter()
        .map(|id| {
            id.as_str()
                .filter(|id| !id.is_empty())
                .map(str::to_owned)
                .ok_or_else(|| format!("{LEDGER}: {id} is not an id"))
        })
        .collect()
}

fn retired_entry(entry: &Value) -> Result<Retired, String> {
    let shown = || format!("{RETIRED}: the retired id {entry}");
    let from =
        entry["from"].as_str().filter(|from| !from.is_empty()).ok_or_else(|| format!("{} has no from", shown()))?;
    let to = match entry.get("to") {
        Some(Value::String(to)) if !to.is_empty() => Successor::By(to.clone()),
        Some(Value::Null) => {
            let reason = entry["reason"].as_str().filter(|reason| !reason.trim().is_empty());
            let reason =
                reason.ok_or_else(|| format!("{} is removed with \"to\": null but says no reason", shown()))?;
            Successor::Removed { reason: reason.to_owned() }
        }
        _ => return Err(format!("{} says no to: name what replaces it, or \"to\": null with a reason", shown())),
    };
    Ok(Retired { from: from.to_owned(), to })
}

pub fn retired_of(text: &str) -> Result<Vec<Retired>, String> {
    let value: Value = serde_json::from_str(text).map_err(|error| format!("{RETIRED} does not read: {error}"))?;
    match value.get("ids") {
        None => Ok(Vec::new()),
        Some(Value::Array(listed)) => listed.iter().map(retired_entry).collect(),
        Some(_) => Err(format!("{RETIRED}: ids is not a list")),
    }
}

pub fn ledger_text(ids: &BTreeSet<String>) -> String {
    let text = serde_json::to_string_pretty(&json!({ "schema_version": 1, "ids": ids })).unwrap_or_default();
    format!("{text}\n")
}

fn read(root: &Path, path: &str) -> Result<String, String> {
    std::fs::read_to_string(root.join(path)).map_err(|error| format!("{path} does not read: {error}"))
}

pub fn findings(root: &Path, current: &BTreeSet<String>) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let ledger = ids_of(&read(root, LEDGER)?)?;
    let retired = retired_of(&read(root, RETIRED)?)?;
    let before = crate::history::committed(root, LEDGER, ids_of)?;
    Ok(check(&Ledgers { current, ledger: &ledger, before: &before, retired: &retired }))
}

pub fn publish(root: &Path, current: &BTreeSet<String>) -> Result<usize, Box<dyn std::error::Error>> {
    let mut ledger = ids_of(&read(root, LEDGER)?)?;
    let before = ledger.len();
    ledger.extend(current.iter().cloned());
    std::fs::write(root.join(LEDGER), ledger_text(&ledger))?;
    Ok(ledger.len() - before)
}

#[cfg(test)]
mod tests;
