use std::collections::BTreeSet;

use domain::connection::Capability;
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const EXPORT: &str = "events";
pub const POLL: &str = "poll";
pub const PUSH: &str = "push";
pub const NO_BACKFILL: &str = "none";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EventSpec {
    pub name: String,
    #[serde(default)]
    pub label: String,
    pub capability: Capability,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub source: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub aliases: Vec<String>,
    #[serde(default)]
    pub schema: Value,
    #[serde(default)]
    pub filters: Vec<String>,
    #[serde(default = "polled")]
    pub cadence: String,
    #[serde(default = "nothing_back")]
    pub backfill: String,
}

fn polled() -> String {
    POLL.to_owned()
}

fn nothing_back() -> String {
    NO_BACKFILL.to_owned()
}

impl EventSpec {
    pub fn answers(&self, kind: &str) -> bool {
        self.name == kind || self.aliases.iter().any(|alias| alias == kind)
    }

    pub fn published(&self) -> String {
        format!("event:{}", self.name)
    }

    pub fn fields(&self) -> Vec<&str> {
        self.schema["properties"]
            .as_object()
            .map(|fields| fields.keys().map(String::as_str).collect())
            .unwrap_or_default()
    }
}

pub fn namespace(kind: &str) -> &str {
    kind.split_once('.').map_or(kind, |(namespace, _)| namespace)
}

fn backfill_reads(backfill: &str) -> bool {
    if backfill == NO_BACKFILL {
        return true;
    }
    let Some(unit) = backfill.chars().last() else { return false };
    let digits = &backfill[..backfill.len() - unit.len_utf8()];
    matches!(unit, 'm' | 'h' | 'd') && !digits.is_empty() && digits.chars().all(|digit| digit.is_ascii_digit())
}

fn checked(key: &str, spec: &EventSpec, seen: &mut BTreeSet<String>) -> Result<(), String> {
    let shown = format!("{key}: the event {}", spec.name);
    let named = spec.name.strip_prefix(key).and_then(|rest| rest.strip_prefix('.'));
    if named.is_none_or(|rest| rest.is_empty() || rest.ends_with('.')) {
        return Err(format!("{shown} must be named {key}.<event>"));
    }
    if spec.capability.as_str().is_empty() {
        return Err(format!("{shown} says no capability"));
    }
    if !seen.insert(spec.name.clone()) {
        return Err(format!("{shown} is declared twice"));
    }
    if let Some(alias) = spec.aliases.iter().find(|alias| namespace(alias) == key || !alias.contains('.')) {
        return Err(format!("{shown} lists {alias} as an old name, but an old name lives outside {key}."));
    }
    let fields = spec.fields();
    if let Some(filter) = spec.filters.iter().find(|filter| !fields.contains(&filter.as_str())) {
        return Err(format!("{shown} filters by {filter}, which its schema does not declare"));
    }
    if spec.cadence != POLL && spec.cadence != PUSH {
        return Err(format!("{shown} has cadence {}, expected {POLL} or {PUSH}", spec.cadence));
    }
    if !backfill_reads(&spec.backfill) {
        return Err(format!("{shown} has backfill {}, expected {NO_BACKFILL} or a window like 7d", spec.backfill));
    }
    Ok(())
}

pub fn read(key: &str, declared: &Value) -> Result<Vec<EventSpec>, String> {
    if declared.is_null() {
        return Ok(Vec::new());
    }
    let mut listed: Vec<EventSpec> =
        serde_json::from_value(declared.clone()).map_err(|error| format!("{key}: events() does not read: {error}"))?;
    let mut seen = BTreeSet::new();
    for spec in &mut listed {
        if spec.label.trim().is_empty() {
            spec.label.clone_from(&spec.name);
        }
        checked(key, spec, &mut seen)?;
    }
    Ok(listed)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn one(spec: Value) -> Result<Vec<EventSpec>, String> {
        read("acme", &json!([spec]))
    }

    fn base() -> Value {
        json!({
            "name": "acme.ticket.opened",
            "capability": "tracker",
            "aliases": ["tracker.issue.created"],
            "schema": { "type": "object", "properties": { "title": { "type": "string" } } },
            "filters": ["title"]
        })
    }

    #[test]
    fn an_event_reads_with_defaults_and_answers_its_old_name() {
        let listed = one(base()).expect("reads");
        assert_eq!(listed[0].cadence, POLL);
        assert_eq!(listed[0].backfill, NO_BACKFILL);
        assert_eq!(listed[0].label, "acme.ticket.opened");
        assert!(listed[0].answers("tracker.issue.created"));
        assert!(!listed[0].answers("tracker.issue"));
        assert_eq!(listed[0].published(), "event:acme.ticket.opened");
        assert_eq!(read("acme", &Value::Null), Ok(Vec::new()));
    }

    #[test]
    fn a_wrong_event_says_what_is_wrong() {
        let wrong = |field: &str, value: Value| {
            let mut spec = base();
            spec[field] = value;
            one(spec).expect_err("refused")
        };
        assert!(wrong("name", json!("ticket.opened")).contains("must be named acme.<event>"));
        assert!(wrong("name", json!("acme.")).contains("must be named"));
        assert!(wrong("filters", json!(["status"])).contains("filters by status"));
        assert!(wrong("cadence", json!("hourly")).contains("cadence hourly"));
        assert!(wrong("backfill", json!("forever")).contains("backfill forever"));
        assert!(wrong("aliases", json!(["acme.old"])).contains("old name"));
        assert!(one(json!({ "name": "acme.x", "capability": "" })).expect_err("refused").contains("no capability"));
        let twice = read("acme", &json!([base(), base()])).expect_err("refused");
        assert!(twice.contains("declared twice"));
        let mut window = base();
        window["backfill"] = json!("7d");
        assert!(one(window).is_ok());
    }
}
