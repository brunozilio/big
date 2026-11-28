use std::sync::{PoisonError, RwLock};

use domain::permission::{Allowed, Behavior, Grant, Rule, allowed};
use serde::Serialize;
use serde_json::{Value, json};
use ::store::Store;

pub(super) const KEY: &str = "permissions.allowed";
const GOOD: &str = "permissions.allowed.good";
const SEEDED: &str = "permissions.defaults";
const VERSION: u64 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Fallback {
    LastGood,
    Closed,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Problem {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fallback: Option<Fallback>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub dropped: Vec<Rule>,
}

impl Problem {
    fn closes(&self) -> bool {
        self.fallback == Some(Fallback::Closed)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Kept {
    pub allowed: Allowed,
    pub problem: Option<Problem>,
}

struct Decoded {
    allowed: Allowed,
    dropped: Vec<Rule>,
}

enum Read {
    Missing,
    Found(Decoded),
    Broken(String),
}

static PROBLEM: RwLock<Option<Problem>> = RwLock::new(None);

pub fn problem() -> Option<Problem> {
    PROBLEM.read().unwrap_or_else(PoisonError::into_inner).clone()
}

pub(super) fn set_problem(problem: Option<Problem>) {
    *PROBLEM.write().unwrap_or_else(PoisonError::into_inner) = problem;
}

fn decoded(value: Value) -> Result<Decoded, String> {
    let grants = match value {
        Value::Array(_) => value,
        Value::Object(mut fields) => match fields.get("schema_version").and_then(Value::as_u64) {
            Some(VERSION) => fields.remove("grants").unwrap_or(Value::Null),
            other => return Err(format!("schema_version {other:?} is not {VERSION}")),
        },
        other => return Err(format!("expected the saved grants, found {other}")),
    };
    let rules = serde_json::from_value::<Vec<Rule>>(grants).map_err(|error| error.to_string())?;
    let dropped: Vec<Rule> = rules.iter().filter(|rule| rule.loose()).cloned().collect();
    if !dropped.is_empty() {
        tracing::warn!(?dropped, "a saved grant for everything is left out");
    }
    Ok(Decoded { allowed: Allowed::of(rules), dropped })
}

fn encoded(allowed: &Allowed) -> Value {
    json!({ "schema_version": VERSION, "grants": allowed })
}

fn read(store: &Store, key: &str) -> Read {
    match store.preference(key) {
        Ok(None) => Read::Missing,
        Ok(Some(value)) => decoded(value).map_or_else(Read::Broken, Read::Found),
        Err(error) => Read::Broken(error.to_string()),
    }
}

fn closed() -> Allowed {
    Allowed::of([
        Rule::new(Grant::Command("*".into()), Behavior::Deny),
        Rule::new(Grant::Site("*".into()), Behavior::Deny),
    ])
}

fn told(problem: Problem) -> Option<Problem> {
    (problem != Problem::default()).then_some(problem)
}

pub fn loaded(store: &Store) -> Kept {
    let error = match read(store, KEY) {
        Read::Missing => return Kept { allowed: Allowed::default(), problem: None },
        Read::Found(Decoded { allowed, dropped }) => {
            return Kept { allowed, problem: told(Problem { dropped, ..Problem::default() }) };
        }
        Read::Broken(error) => error,
    };
    tracing::error!(%error, "the saved permission list does not load");
    let error = Some(error);
    match read(store, GOOD) {
        Read::Found(Decoded { allowed, dropped }) => {
            Kept { allowed, problem: told(Problem { fallback: Some(Fallback::LastGood), error, dropped }) }
        }
        _ => Kept { allowed: closed(), problem: told(Problem { fallback: Some(Fallback::Closed), error, dropped: Vec::new() }) },
    }
}

pub fn kept(store: &Store) -> Allowed {
    loaded(store).allowed
}

pub(super) fn saved(store: &Store, allowed: &Allowed) -> store::Result<()> {
    let value = encoded(allowed);
    store.set_preference(KEY, &value).and_then(|()| store.set_preference(GOOD, &value))
}

pub(super) fn persisted(store: &Store, problem: Option<&Problem>, changed: &Allowed) -> bool {
    if problem.is_some_and(Problem::closes) {
        return false;
    }
    if let Err(error) = saved(store, changed) {
        tracing::warn!(%error, "the allowed list could not be saved");
        return false;
    }
    true
}

pub(super) fn seeded(store: &Store) -> Kept {
    let mut kept = loaded(store);
    if kept.problem.is_some() || store.preference(SEEDED).ok().flatten().is_some() {
        return kept;
    }
    for grant in allowed::defaults() {
        kept.allowed.add(grant);
    }
    let written = saved(store, &kept.allowed).and_then(|()| store.set_preference(SEEDED, &json!(true)));
    if let Err(error) = written {
        tracing::warn!(%error, "the default allowed list could not be saved");
    }
    kept
}

#[cfg(test)]
#[path = "store_tests.rs"]
mod tests;
