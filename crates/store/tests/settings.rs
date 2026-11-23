use serde_json::json;
use store::{ConnectionRecord, Store};

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

#[test]
fn an_external_event_is_seen_once_per_connection() {
    let store = Store::in_memory(0).expect("the store answers");
    assert!(!store.was_seen("c1", "ev-1@3").expect("the store answers"));
    assert!(store.mark_seen("c1", "ev-1@3").expect("the store answers"));
    assert!(!store.mark_seen("c1", "ev-1@3").expect("the store answers"));
    assert!(store.was_seen("c1", "ev-1@3").expect("the store answers"));
    assert!(!store.was_seen("c2", "ev-1@3").expect("the store answers"));
}

#[test]
fn each_connection_keeps_its_own_cursor() {
    let store = Store::in_memory(0).expect("the store answers");
    assert_eq!(store.cursor("c1").expect("the store answers"), None);
    store.set_cursor("c1", "2026-01-01T00:00:00Z").expect("the store answers");
    store.set_cursor("c2", "etag-9").expect("the store answers");
    store.set_cursor("c1", "2026-02-01T00:00:00Z").expect("the store answers");
    assert_eq!(store.cursor("c1").expect("the store answers").as_deref(), Some("2026-02-01T00:00:00Z"));
    assert_eq!(store.cursor("c2").expect("the store answers").as_deref(), Some("etag-9"));
}

#[test]
fn connections_are_saved_updated_listed_and_deleted() {
    let store = Store::in_memory(0).expect("the store answers");
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

#[test]
fn branding_is_a_set_of_named_values() {
    let store = Store::in_memory(0).expect("the store answers");
    assert!(store.branding().expect("the store answers").is_empty());
    store.set_branding("name", &json!("Anything")).expect("the store answers");
    store.set_branding("logo", &json!({"svg": "<svg/>"})).expect("the store answers");
    store.set_branding("name", &json!("Other")).expect("the store answers");
    let branding = store.branding().expect("the store answers");
    assert_eq!(branding.get("name"), Some(&json!("Other")));
    assert_eq!(branding.get("logo"), Some(&json!({"svg": "<svg/>"})));
    assert_eq!(branding.len(), 2);
}
