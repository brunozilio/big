use std::sync::{Arc, PoisonError, RwLock};

use axum::Json;
use axum::extract::State;
use axum::response::Response;
use domain::permission::{Allowed, Behavior, Grant, Rule, allowed};
use serde::Serialize;
use serde_json::{Value, json};
use store::Store;

use crate::error::{ApiError, ApiResult, json as reply, said};
use crate::state::Ctx;

const KEY: &str = "permissions.allowed";
const GOOD: &str = "permissions.allowed.good";
const SEEDED: &str = "permissions.defaults";
const VERSION: u64 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Fallback {
    LastGood,
    Closed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Problem {
    pub fallback: Fallback,
    pub error: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Kept {
    pub allowed: Allowed,
    pub problem: Option<Problem>,
}

enum Read {
    Missing,
    Found(Allowed),
    Broken(String),
}

static PROBLEM: RwLock<Option<Problem>> = RwLock::new(None);

fn problem() -> Option<Problem> {
    PROBLEM.read().unwrap_or_else(PoisonError::into_inner).clone()
}

fn set_problem(problem: Option<Problem>) {
    *PROBLEM.write().unwrap_or_else(PoisonError::into_inner) = problem;
}

fn decoded(value: Value) -> Result<Allowed, String> {
    let grants = match value {
        Value::Array(_) => value,
        Value::Object(mut fields) => match fields.get("schema_version").and_then(Value::as_u64) {
            Some(VERSION) => fields.remove("grants").unwrap_or(Value::Null),
            other => return Err(format!("schema_version {other:?} is not {VERSION}")),
        },
        other => return Err(format!("expected the saved grants, found {other}")),
    };
    let rules = serde_json::from_value::<Vec<Rule>>(grants).map_err(|error| error.to_string())?;
    let loose: Vec<&Rule> = rules.iter().filter(|rule| rule.loose()).collect();
    if !loose.is_empty() {
        tracing::warn!(?loose, "a saved grant for everything is left out");
    }
    Ok(Allowed::of(rules))
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

pub fn loaded(store: &Store) -> Kept {
    let error = match read(store, KEY) {
        Read::Missing => return Kept { allowed: Allowed::default(), problem: None },
        Read::Found(allowed) => return Kept { allowed, problem: None },
        Read::Broken(error) => error,
    };
    tracing::error!(%error, "the saved permission list does not load");
    match read(store, GOOD) {
        Read::Found(allowed) => Kept { allowed, problem: Some(Problem { fallback: Fallback::LastGood, error }) },
        _ => Kept { allowed: closed(), problem: Some(Problem { fallback: Fallback::Closed, error }) },
    }
}

pub fn kept(store: &Store) -> Allowed {
    loaded(store).allowed
}

fn saved(store: &Store, allowed: &Allowed) -> store::Result<()> {
    let value = encoded(allowed);
    store.set_preference(KEY, &value).and_then(|()| store.set_preference(GOOD, &value))
}

fn seeded(store: &Store) -> Kept {
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

pub fn install(store: Arc<Store>) {
    let Kept { allowed: initial, problem: told } = seeded(&store);
    if told.is_none()
        && let Err(error) = saved(&store, &initial)
    {
        tracing::warn!(%error, "the allowed list could not be saved");
    }
    set_problem(told);
    allowed::install(
        initial,
        Arc::new(move |changed: &Allowed| {
            if problem().is_some_and(|problem| problem.fallback == Fallback::Closed) {
                return;
            }
            if let Err(error) = saved(&store, changed) {
                tracing::warn!(%error, "the allowed list could not be saved");
            }
        }),
    );
}

fn listed(allowed: &Allowed) -> Value {
    json!({ "grants": allowed, "problem": problem() })
}

fn current(store: &Store) -> Allowed {
    allowed::global().unwrap_or_else(|| kept(store))
}

fn refused_loose(rules: &[Rule]) -> ApiResult<()> {
    let Some(loose) = rules.iter().find(|rule| rule.loose()) else { return Ok(()) };
    let value = match &loose.grant {
        Grant::Command(value) | Grant::Site(value) | Grant::Tool(value) => value.trim().to_owned(),
    };
    Err(ApiError::bad(said(
        format!("{value} would allow everything; name the commands or sites, or deny it instead."),
        format!("{value} permitiría todo; nombra los comandos o sitios, o deniégalo."),
        format!("{value} liberaria tudo; nomeie os comandos ou sites, ou negue."),
    )))
}

pub fn replaced(store: &Store, body: &Value) -> ApiResult<Allowed> {
    let grants = serde_json::from_value::<Vec<Rule>>(body["grants"].clone()).map_err(|_| {
        ApiError::bad(said(
            "Send grants as a list of { kind: command | site | tool, value, behavior: allow | ask | deny }.",
            "Envía grants como una lista de { kind: command | site | tool, value, behavior: allow | ask | deny }.",
            "Envie grants como uma lista de { kind: command | site | tool, value, behavior: allow | ask | deny }.",
        ))
    })?;
    refused_loose(&grants)?;
    let wanted = Allowed::of(grants);
    set_problem(None);
    let changed = allowed::change(|allowed| *allowed = wanted.clone());
    match changed {
        Some(changed) => Ok(changed),
        None => {
            saved(store, &wanted)?;
            Ok(wanted)
        }
    }
}

pub async fn get(State(ctx): State<Ctx>) -> Json<Value> {
    Json(listed(&current(&ctx.backend.store)))
}

pub async fn put(State(ctx): State<Ctx>, Json(body): Json<Value>) -> ApiResult<Response> {
    let saved = replaced(&ctx.backend.store, &body)?;
    Ok(reply(listed(&saved)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_list_is_tidied_and_kept_in_the_store() {
        let store = Store::in_memory(0).expect("store");
        let body = json!({ "grants": [
            { "kind": "command", "value": "  cargo   test " },
            { "kind": "site", "value": "https://www.gov.br/pt-br" },
            { "kind": "site", "value": "www.gov.br" },
            { "kind": "tool", "value": "" },
            { "kind": "command", "value": "git push", "behavior": "deny" },
        ] });
        let saved = replaced(&store, &body).expect("saved");
        assert_eq!(
            saved.rules(),
            [
                Rule::from(Grant::Command("cargo test".into())),
                Rule::from(Grant::Site("www.gov.br".into())),
                Rule::new(Grant::Command("git push".into()), Behavior::Deny),
            ]
        );
        assert_eq!(kept(&store), saved);
        assert!(replaced(&store, &json!({ "grants": [{ "kind": "nope", "value": "x" }] })).is_err());
    }

    #[test]
    fn the_defaults_come_once_and_keep_what_was_saved() {
        let store = Store::in_memory(0).expect("store");
        replaced(&store, &json!({ "grants": [{ "kind": "site", "value": "gov.br" }] })).expect("saved");
        let first = seeded(&store).allowed;
        assert!(first.has(&Grant::Site("gov.br".into())));
        assert!(first.has(&Grant::Command("git status".into())));
        replaced(&store, &json!({ "grants": [] })).expect("emptied");
        assert!(seeded(&store).allowed.rules().is_empty());
    }

    fn denied_push() -> Value {
        json!({ "grants": [{ "kind": "command", "value": "git push", "behavior": "deny" }] })
    }

    #[test]
    fn a_broken_list_keeps_the_last_good_denials_and_says_why() {
        let store = Store::in_memory(0).expect("store");
        let good = replaced(&store, &denied_push()).expect("saved");
        store.set_preference(KEY, &json!({ "grants": "oops" })).expect("broken");
        let kept = loaded(&store);
        assert_eq!(kept.allowed, good);
        let problem = kept.problem.expect("a problem");
        assert_eq!(problem.fallback, Fallback::LastGood);
        assert!(problem.error.contains("schema_version"), "{}", problem.error);
        assert!(seeded(&store).problem.is_some(), "the defaults do not cover a broken list");
    }

    #[test]
    fn a_broken_list_with_no_good_copy_closes_commands_and_sites() {
        let store = Store::in_memory(0).expect("store");
        store.set_preference(KEY, &json!({ "schema_version": 1, "grants": [{ "kind": 5 }] })).expect("broken");
        let kept = loaded(&store);
        assert_eq!(kept.problem.map(|problem| problem.fallback), Some(Fallback::Closed));
        let effects = [domain::permission::Effect::exec("ls")];
        let call = domain::permission::Call { tool: "Bash", subject: "ls", effects: &effects };
        assert_eq!(kept.allowed.verdict(&call), Some(Behavior::Deny));
    }

    #[test]
    fn an_old_plain_list_still_loads_and_an_unknown_version_does_not() {
        let store = Store::in_memory(0).expect("store");
        store.set_preference(KEY, &json!([{ "kind": "site", "value": "gov.br" }])).expect("old");
        assert!(kept(&store).has(&Grant::Site("gov.br".into())));
        store.set_preference(KEY, &json!({ "schema_version": 9, "grants": [] })).expect("newer");
        assert!(loaded(&store).problem.is_some());
    }

    #[test]
    fn a_grant_for_everything_is_refused() {
        let store = Store::in_memory(0).expect("store");
        for (kind, value) in [("site", "*"), ("command", "*"), ("site", "*.com")] {
            let body = json!({ "grants": [{ "kind": kind, "value": value }] });
            assert!(replaced(&store, &body).is_err(), "{kind} {value}");
        }
        let denied = json!({ "grants": [{ "kind": "site", "value": "*", "behavior": "deny" }] });
        assert_eq!(replaced(&store, &denied).expect("a denial for everything is fine").rules().len(), 1);
        assert!(replaced(&store, &json!({ "grants": [{ "kind": "site", "value": "*.gov.br" }] })).is_ok());
    }
}
