use domain::connection::ConnectionId;
use serde_json::json;

use super::*;

const PACE: Pace = Pace { approval: 10, execution: 20 };

fn args() -> Value {
    json!({ "title": "Pacote 131", "environment": "hext", "window": "2026-11-26T22:00" })
}

#[test]
fn a_change_waits_for_approval_then_executes_then_is_done() {
    assert_eq!(PACE.status(100, 105), AWAITING);
    assert_eq!(PACE.status(100, 110), EXECUTING);
    assert_eq!(PACE.status(100, 130), DONE);
}

#[test]
fn creating_twice_with_the_same_key_returns_the_first_change() {
    let mut ledger = Ledger::default();
    let first = run(actions::CREATE, &args(), "k1", &mut ledger, PACE, 100);
    let again = run(actions::CREATE, &args(), "k1", &mut ledger, PACE, 105);
    let Ok(first) = first else { panic!("{first:?}") };
    let Ok(again) = again else { panic!("{again:?}") };
    assert_eq!(first["ref"], again["ref"]);
    assert_eq!((first["created"].clone(), again["created"].clone()), (json!(true), json!(false)));
    assert_eq!(first["key"], "GMUD-4831");
    assert_eq!(first["status"], AWAITING);
    let other = run(actions::CREATE, &args(), "k2", &mut ledger, PACE, 105).unwrap_or_default();
    assert_ne!(other["ref"], first["ref"]);
    let missing = run(actions::CREATE, &json!({ "title": "x" }), "k3", &mut ledger, PACE, 105);
    assert!(missing.is_err());
}

#[test]
fn an_update_keeps_the_new_data_and_counts_revisions_until_cancelled() {
    let mut ledger = Ledger::default();
    let made = run(actions::CREATE, &args(), "k1", &mut ledger, PACE, 100).unwrap_or_default();
    let change = made["ref"].clone();
    let update = json!({ "change": change, "window": "2026-11-27T22:00", "tasks": [{ "title": "Deploy" }] });
    let updated = run(actions::UPDATE, &update, "", &mut ledger, PACE, 101).unwrap_or_default();
    assert_eq!((updated["window"].clone(), updated["revision"].clone()), (json!("2026-11-27T22:00"), json!(1)));
    let read = run(actions::GET, &json!({ "change": change }), "", &mut ledger, PACE, 115).unwrap_or_default();
    assert_eq!(read["status"], EXECUTING);
    let cancelled = run(actions::CANCEL, &json!({ "change": change }), "", &mut ledger, PACE, 116).unwrap_or_default();
    assert_eq!(cancelled["status"], CANCELLED);
    assert!(run(actions::UPDATE, &update, "", &mut ledger, PACE, 117).is_err());
}

#[test]
fn a_change_unknown_after_a_restart_is_read_back_from_its_reference() {
    let mut ledger = Ledger::default();
    let read = run(actions::GET, &json!({ "change": "mock-100-7" }), "", &mut ledger, PACE, 200).unwrap_or_default();
    assert_eq!((read["key"].clone(), read["status"].clone()), (json!("GMUD-4837"), json!(DONE)));
    assert!(run(actions::GET, &json!({ "change": "GMUD-1" }), "", &mut ledger, PACE, 200).is_err());
}

#[test]
fn every_status_turn_between_two_polls_becomes_one_event() {
    let mut ledger = Ledger::default();
    let made = run(actions::CREATE, &args(), "k1", &mut ledger, PACE, 100).unwrap_or_default();
    let creds = Credentials::new(&ConnectionId::from("c1"), "mock", json!({}));
    let first = events_between(&ledger, &creds, PACE, 100, 115);
    assert_eq!(first.len(), 1);
    assert_eq!((first[0].kind.as_str(), first[0].subject.clone()), (actions::STATUS_CHANGED, made["ref"].as_str().unwrap_or_default().to_owned()));
    assert_eq!(first[0].data["status"], EXECUTING);
    let later = events_between(&ledger, &creds, PACE, 115, 200);
    assert_eq!(later.iter().map(|event| event.data["status"].clone()).collect::<Vec<_>>(), [json!(DONE)]);
    assert!(events_between(&ledger, &creds, PACE, 200, 300).is_empty());
}

#[test]
fn the_simulated_system_connects_with_any_name() {
    let AuthSpec::Token(spec) = MockChanges.descriptor().auth else { panic!("not a token") };
    assert_eq!(spec.access_token(&json!({ "name": "teste" }), Language::Portuguese), Ok("teste".to_owned()));
}
