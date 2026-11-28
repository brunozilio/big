#![allow(clippy::expect_used, reason = "a failed setup step stops the test")]

#[macro_use]
mod common;

use common::Fresh;
use serde_json::json;
use store::ConnectionRecord;

fn connection(id: &str, provider: &str) -> ConnectionRecord {
    ConnectionRecord {
        id: id.into(),
        provider: provider.into(),
        label: format!("{provider} work"),
        account: Some("someone".into()),
        capabilities: vec!["tracker".into(), "code".into()],
        settings: json!({"site": "x"}),
    }
}

fn an_external_event_is_seen_once_per_connection(fresh: Fresh) {
    let place = fresh();
    let store = place.open(0);
    assert!(!store.was_seen("c1", "ev-1@3").expect("the store answers"));
    assert!(store.mark_seen("c1", "ev-1@3").expect("the store answers"));
    assert!(!store.mark_seen("c1", "ev-1@3").expect("the store answers"));
    assert!(store.was_seen("c1", "ev-1@3").expect("the store answers"));
    assert!(!store.was_seen("c2", "ev-1@3").expect("the store answers"));
}

fn each_connection_keeps_its_own_cursor(fresh: Fresh) {
    let place = fresh();
    let store = place.open(0);
    assert_eq!(store.cursor("c1").expect("the store answers"), None);
    store.set_cursor("c1", "2026-01-01T00:00:00Z").expect("the store answers");
    store.set_cursor("c2", "etag-9").expect("the store answers");
    store.set_cursor("c1", "2026-02-01T00:00:00Z").expect("the store answers");
    assert_eq!(store.cursor("c1").expect("the store answers").as_deref(), Some("2026-02-01T00:00:00Z"));
    assert_eq!(store.cursor("c2").expect("the store answers").as_deref(), Some("etag-9"));
}

fn connections_are_saved_updated_listed_and_deleted(fresh: Fresh) {
    let place = fresh();
    let store = place.open(0);
    store.save_connection(&connection("c1", "alpha")).expect("the store answers");
    store.save_connection(&connection("c2", "beta")).expect("the store answers");
    let renamed = ConnectionRecord { label: "renamed".into(), account: None, ..connection("c1", "alpha") };
    store.save_connection(&renamed).expect("the store answers");
    assert_eq!(store.connection("c1").expect("the store answers"), Some(renamed.clone()));
    assert_eq!(store.connections().expect("the store answers"), [renamed, connection("c2", "beta")]);
    assert!(store.delete_connection("c2").expect("the store answers"));
    assert!(!store.delete_connection("c2").expect("the store answers"));
    assert_eq!(store.connection("c2").expect("the store answers"), None);
    assert_eq!(store.connections().expect("the store answers").len(), 1);
}

fn branding_is_a_set_of_named_values(fresh: Fresh) {
    let place = fresh();
    let store = place.open(0);
    assert!(store.branding().expect("the store answers").is_empty());
    store.set_branding("name", &json!("Anything")).expect("the store answers");
    store.set_branding("logo", &json!({"svg": "<svg/>"})).expect("the store answers");
    store.set_branding("name", &json!("Other")).expect("the store answers");
    let branding = store.branding().expect("the store answers");
    assert_eq!(branding.get("name"), Some(&json!("Other")));
    assert_eq!(branding.get("logo"), Some(&json!({"svg": "<svg/>"})));
    assert_eq!(branding.len(), 2);
}

fn a_preference_is_kept_and_replaced(fresh: Fresh) {
    let place = fresh();
    let store = place.open(0);
    assert_eq!(store.preference("browser.enabled").expect("the store answers"), None);
    store.set_preference("browser.enabled", &json!(false)).expect("the store answers");
    store.set_preference("browser.enabled", &json!(true)).expect("the store answers");
    assert_eq!(store.preference("browser.enabled").expect("the store answers"), Some(json!(true)));
}

fn the_language_defaults_to_portuguese_and_keeps_a_known_code(fresh: Fresh) {
    let place = fresh();
    let store = place.open(0);
    assert_eq!(store.language().expect("the store answers"), domain::Language::Portuguese);
    store.set_branding("language", &json!("es")).expect("the store answers");
    assert_eq!(store.language().expect("the store answers"), domain::Language::Spanish);
    store.set_branding("language", &json!("fr")).expect("the store answers");
    assert_eq!(store.language().expect("the store answers"), domain::Language::Portuguese);
}

scenarios!(
    an_external_event_is_seen_once_per_connection,
    each_connection_keeps_its_own_cursor,
    connections_are_saved_updated_listed_and_deleted,
    branding_is_a_set_of_named_values,
    a_preference_is_kept_and_replaced,
    the_language_defaults_to_portuguese_and_keeps_a_known_code,
);
