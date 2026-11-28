#![allow(clippy::expect_used, reason = "a failed setup step stops the test")]

#[macro_use]
mod common;

use common::Fresh;
use domain::factory::{Finding, FindingKind};
use domain::workflow::ItemId;
use serde_json::json;
use store::{BrowserDevice, Store};

fn finding(id: &str, kind: FindingKind, generation: &str) -> Finding {
    Finding {
        id: id.into(),
        item: ItemId::from("i1"),
        workflow: "entrega".into(),
        generation: Some(generation.into()),
        kind,
        step: "checks".into(),
        gate: None,
        caught_by: None,
        detail: json!({}),
        at: 1,
    }
}

fn device(id: &str, owner: &str) -> BrowserDevice {
    BrowserDevice { device: id.into(), owner: owner.into(), name: format!("Chrome {id}"), created: 1 }
}

#[test]
fn a_store_kept_in_memory_answers_like_a_file() {
    let store = Store::in_memory(0).expect("store");
    store.set_connector_memory("a", "ledger", "[1]").expect("write");
    assert_eq!(store.connector_memory("a", "ledger").expect("read"), Some("[1]".to_owned()));
    assert_eq!(store.database().dialect(), database::Dialect::Sqlite);
}

fn an_item_is_archived_once_and_can_come_back(fresh: Fresh) {
    let place = fresh();
    let store = place.open(0);
    store.archive_item("i1", 10).expect("archive");
    store.archive_item("i1", 20).expect("again");
    assert_eq!(store.archived_items().expect("list").get("i1"), Some(&10), "the first time stays");
    store.unarchive_item("i1").expect("unarchive");
    assert!(store.archived_items().expect("list").is_empty());
}

fn a_connector_memory_is_kept_per_connection_and_name(fresh: Fresh) {
    let place = fresh();
    let store = place.open(0);
    assert_eq!(store.connector_memory("a", "ledger").expect("read"), None);
    store.set_connector_memory("a", "ledger", "[1]").expect("write");
    store.set_connector_memory("a", "ledger", "[2]").expect("overwrite");
    store.set_connector_memory("b", "ledger", "[9]").expect("other connection");
    assert_eq!(store.connector_memory("a", "ledger").expect("read"), Some("[2]".to_owned()));
    assert_eq!(store.connector_memory("b", "ledger").expect("read"), Some("[9]".to_owned()));
    assert_eq!(store.connector_memory("a", "other").expect("read"), None);
}

fn a_finding_is_recorded_once_and_only_the_new_ones_come_back(fresh: Fresh) {
    let place = fresh();
    let store = place.open(0);
    let first = store.record_findings(&[finding("a", FindingKind::Failure, "g1")]).expect("record");
    assert_eq!(first.len(), 1);
    let again = store
        .record_findings(&[finding("a", FindingKind::Failure, "g1"), finding("b", FindingKind::Escape, "g1")])
        .expect("record");
    assert_eq!(again.iter().map(|found| found.id.as_str()).collect::<Vec<_>>(), vec!["b"]);
    assert_eq!(store.findings(Some(FindingKind::Failure)).expect("list").len(), 1);
    assert_eq!(store.findings(Some(FindingKind::Escape)).expect("list").len(), 1);
    let all = store.findings(None).expect("list");
    assert_eq!(all.iter().map(|found| found.id.as_str()).collect::<Vec<_>>(), vec!["a", "b"]);
}

fn a_generation_is_recorded_once_and_loads_back(fresh: Fresh) {
    let place = fresh();
    let store = place.open(0);
    assert!(store.record_generation("g1", &["base".into()], &json!({"agents/a.md": "h"})).expect("record"));
    assert!(!store.record_generation("g1", &[], &json!({})).expect("record"), "a version is immutable");
    let kept = store.generation("g1").expect("load").expect("found");
    assert_eq!(kept.chain, vec!["base".to_owned()]);
    assert_eq!(kept.files, json!({"agents/a.md": "h"}));
}

fn a_paired_browser_is_found_by_its_token_and_pairing_again_replaces_the_old_token(fresh: Fresh) {
    let place = fresh();
    let store = place.open(0);
    store.add_browser_device("t1", &device("a", "ana")).expect("pair");
    assert_eq!(store.browser_device("t1").expect("read"), Some(device("a", "ana")));
    store.add_browser_device("t2", &device("a", "ana")).expect("pair again");
    assert_eq!(store.browser_device("t1").expect("read"), None);
    store.add_browser_device("t3", &device("b", "bia")).expect("pair");
    assert_eq!(store.browser_devices_of("ana").expect("list").len(), 1);
    assert!(!store.forget_browser_device("bia", "a").expect("forget"));
    assert!(store.forget_browser_device("ana", "a").expect("forget"));
    assert_eq!(store.browser_device("t2").expect("read"), None);
}

fn people_count_their_open_sessions(fresh: Fresh) {
    let place = fresh();
    let store = place.open(0);
    let ana = domain::principal::Principal {
        user: domain::principal::UserId("u-ana".into()),
        email: "ana@example.com".into(),
        name: "Ana".into(),
    };
    store.ensure_user(&ana, 1).expect("user");
    store.open_session("t1", &ana.user, 1, 100).expect("open");
    store.open_session("t2", &ana.user, 1, 5).expect("open");
    let people = store.people(10).expect("people");
    assert_eq!(people.iter().map(|person| person.sessions).collect::<Vec<_>>(), [1]);
    assert_eq!(store.close_sessions_of(&ana.user).expect("close"), 2);
    assert_eq!(store.user_ids().expect("ids"), ["u-ana"]);
}

fn a_git_token_names_its_owner_and_connection(fresh: Fresh) {
    let place = fresh();
    let store = place.open(0);
    store.put_git_token("g1", "u-ana", "c1").expect("put");
    store.put_git_token("g1", "u-ana", "c2").expect("replace");
    assert_eq!(store.git_token("g1").expect("read"), Some(("u-ana".to_owned(), "c2".to_owned())));
    assert_eq!(store.git_token("g2").expect("read"), None);
}

scenarios!(
    an_item_is_archived_once_and_can_come_back,
    a_connector_memory_is_kept_per_connection_and_name,
    a_finding_is_recorded_once_and_only_the_new_ones_come_back,
    a_generation_is_recorded_once_and_loads_back,
    a_paired_browser_is_found_by_its_token_and_pairing_again_replaces_the_old_token,
    people_count_their_open_sessions,
    a_git_token_names_its_owner_and_connection,
);
