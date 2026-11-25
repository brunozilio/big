use std::sync::Arc;

use domain::Language;
use serde_json::{Value, json};

use super::{creds, loaded};
use crate::descriptor::AuthSpec;
use crate::error::ActionError;
use crate::http::FakeHttp;
use crate::provider::Provider;
use crate::scripted::{Local, ScriptedProvider};

const AWAITING: &str = "awaiting_approval";
const EXECUTING: &str = "executing";
const DONE: &str = "done";
const CANCELLED: &str = "cancelled";

fn pace() -> Value {
    json!({ "approval": 10, "execution": 20 })
}

fn args() -> Value {
    json!({ "title": "Pacote 131", "environment": "hext", "window": "2026-11-26T22:00" })
}

struct Mock {
    provider: ScriptedProvider,
    ledger: Value,
}

impl Mock {
    fn new() -> Mock {
        Mock { provider: loaded("mock_changes"), ledger: Value::Null }
    }

    fn run(&mut self, action: &str, args: &Value, key: &str, now: i64) -> Result<Value, ActionError> {
        let input =
            json!({ "action": action, "args": args, "key": key, "ledger": self.ledger, "pace": pace(), "now": now });
        let out = self.provider.script.invoke_here("run", &input, Arc::new(Local))?;
        self.ledger = out["ledger"].clone();
        Ok(out["value"].clone())
    }

    fn events(&self, since: i64, now: i64) -> Vec<Value> {
        let input = json!({ "ledger": self.ledger, "pace": pace(), "since": since, "now": now });
        let found = self.provider.script.invoke_here("eventsBetween", &input, Arc::new(Local)).unwrap_or_default();
        found.as_array().cloned().unwrap_or_default()
    }
}

#[test]
fn a_change_waits_for_approval_then_executes_then_is_done() {
    let mut mock = Mock::new();
    let at =
        |mock: &mut Mock, now| mock.run("changes.get", &json!({ "change": "mock-100-1" }), "", now).unwrap_or_default();
    assert_eq!(at(&mut mock, 105)["status"], AWAITING);
    assert_eq!(at(&mut mock, 110)["status"], EXECUTING);
    assert_eq!(at(&mut mock, 130)["status"], DONE);
}

#[test]
fn creating_twice_with_the_same_key_returns_the_first_change() {
    let mut mock = Mock::new();
    let first = mock.run("changes.create", &args(), "k1", 100);
    let again = mock.run("changes.create", &args(), "k1", 105);
    let Ok(first) = first else { panic!("{first:?}") };
    let Ok(again) = again else { panic!("{again:?}") };
    assert_eq!(first["ref"], again["ref"]);
    assert_eq!((first["created"].clone(), again["created"].clone()), (json!(true), json!(false)));
    assert_eq!(first["key"], "GMUD-4831");
    assert_eq!(first["status"], AWAITING);
    let other = mock.run("changes.create", &args(), "k2", 105).unwrap_or_default();
    assert_ne!(other["ref"], first["ref"]);
    let missing = mock.run("changes.create", &json!({ "title": "x" }), "k3", 105);
    assert!(missing.is_err());
}

#[test]
fn an_update_keeps_the_new_data_and_counts_revisions_until_cancelled() {
    let mut mock = Mock::new();
    let made = mock.run("changes.create", &args(), "k1", 100).unwrap_or_default();
    let change = made["ref"].clone();
    let update = json!({ "change": change, "window": "2026-11-27T22:00", "tasks": [{ "title": "Deploy" }] });
    let updated = mock.run("changes.update", &update, "", 101).unwrap_or_default();
    assert_eq!((updated["window"].clone(), updated["revision"].clone()), (json!("2026-11-27T22:00"), json!(1)));
    let read = mock.run("changes.get", &json!({ "change": change }), "", 115).unwrap_or_default();
    assert_eq!(read["status"], EXECUTING);
    let cancelled = mock.run("changes.cancel", &json!({ "change": change }), "", 116).unwrap_or_default();
    assert_eq!(cancelled["status"], CANCELLED);
    assert!(mock.run("changes.update", &update, "", 117).is_err());
}

#[test]
fn a_change_unknown_after_a_restart_is_read_back_from_its_reference() {
    let mut mock = Mock::new();
    let read = mock.run("changes.get", &json!({ "change": "mock-100-7" }), "", 200).unwrap_or_default();
    assert_eq!((read["key"].clone(), read["status"].clone()), (json!("GMUD-4837"), json!(DONE)));
    assert!(mock.run("changes.get", &json!({ "change": "GMUD-1" }), "", 200).is_err());
}

#[test]
fn every_status_turn_between_two_polls_becomes_one_event() {
    let mut mock = Mock::new();
    let made = mock.run("changes.create", &args(), "k1", 100).unwrap_or_default();
    let first = mock.events(100, 115);
    assert_eq!(first.len(), 1);
    assert_eq!(
        (first[0]["kind"].clone(), first[0]["subject"].clone()),
        (json!("changes.status.changed"), made["ref"].clone())
    );
    assert_eq!(first[0]["data"]["status"], EXECUTING);
    let later = mock.events(115, 200);
    assert_eq!(later.iter().map(|event| event["data"]["status"].clone()).collect::<Vec<_>>(), [json!(DONE)]);
    assert!(mock.events(200, 300).is_empty());
}

#[test]
fn the_simulated_system_connects_with_any_name() {
    let provider = loaded("mock_changes");
    assert_eq!(provider.descriptor().key.0, "mock-changes");
    let AuthSpec::Token(spec) = provider.descriptor().auth else { panic!("not a token") };
    assert_eq!(spec.access_token(&json!({ "name": "teste" }), Language::Portuguese), Ok("teste".to_owned()));
}

#[tokio::test]
async fn a_change_made_through_the_provider_is_kept_between_calls_and_polled_as_events() {
    let provider = loaded("mock_changes");
    let http = FakeHttp::new();
    let made = provider.call(&http, &creds("mock", json!({})), "changes.create", &args(), "kept-1").await;
    let Ok(made) = made else { panic!("{made:?}") };
    assert_eq!(made["status"], AWAITING);
    let read =
        provider.call(&http, &creds("mock", json!({})), "changes.get", &json!({ "change": made["ref"] }), "").await;
    assert_eq!(read.map(|read| read["key"].clone()), Ok(made["key"].clone()));
    let Some(events) = provider.events() else { panic!("no events") };
    let first = events.poll(&http, &creds("mock", json!({})), None).await;
    let Ok(first) = first else { panic!("{first:?}") };
    assert!(first.events.is_empty() && !first.cursor.is_empty());
    let later = events.poll(&http, &creds("mock", json!({})), Some("0")).await;
    assert!(later.is_ok(), "{later:?}");
    let accounts = provider.accounts(&http, &creds(" ", json!({}))).await.unwrap_or_default();
    assert_eq!(accounts.first().map(|account| account.label.as_str()), Some("mock-changes"));
}
