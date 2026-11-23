#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use connectors::contracts::tracker;
use domain::effect::{Effect, EffectRequest, IdempotencyKey};
use serde_json::json;
use support::{HOST_CONNECTION, TRACKER_CONNECTION, call, created, harness, standard_item};
use sync::{Pushed, push, push_impediment, push_status};

fn key() -> IdempotencyKey {
    IdempotencyKey::new("i1", "review", None, 4, 1)
}

#[tokio::test]
async fn a_status_set_goes_to_the_tracker_of_the_item_with_the_effect_key() {
    let h = harness().await;
    standard_item(&h, "i1");
    let connectors = h.sync_connectors();
    let pushed = push_status(&connectors, &h.store, &h.item("i1"), "Review", &key()).await.unwrap();
    assert_eq!(pushed, Pushed::Done(json!({"status": "Review", "changed": true})));
    let again = push_status(&connectors, &h.store, &h.item("i1"), "Review", &key()).await.unwrap();
    assert_eq!(again, Pushed::Done(json!({"status": "Review", "changed": false})));
    let writes = h.fake.writes();
    assert_eq!(writes[0], (tracker::TRANSITION.into(), json!({"issue": "K-1", "status": "Review"}), key().to_string()));
}

#[tokio::test]
async fn an_impediment_set_carries_its_reason_and_uses_the_connection_that_made_the_issue() {
    let h = harness().await;
    let mut events = vec![created("i1", json!({}))];
    events.extend(call(
        "i1",
        1,
        tracker::CREATE_ISSUE,
        TRACKER_CONNECTION,
        json!({"project": "P"}),
        json!({"key": "P-3"}),
    ));
    h.add("i1", &events);
    let connectors = h.sync_connectors();
    let request = EffectRequest::new(key(), Effect::ImpedimentSet { on: true, reason: Some("waiting".into()) });
    let pushed = push(&connectors, &h.store, &h.item("i1"), &request).await.unwrap();
    assert_eq!(pushed, Pushed::Done(json!({"impediment": true})));
    assert_eq!(h.fake.writes()[0].1, json!({"issue": "P-3", "on": true, "reason": "waiting"}));
    let cleared = push_impediment(&connectors, &h.store, &h.item("i1"), false, None, &key()).await.unwrap();
    assert_eq!(cleared, Pushed::Done(json!({"impediment": false})));
}

#[tokio::test]
async fn items_without_an_issue_or_a_tracker_connection_and_other_effects_are_not_pushed() {
    let h = harness().await;
    h.add("bare", &[created("bare", json!({}))]);
    let connectors = h.sync_connectors();
    let bare = push_status(&connectors, &h.store, &h.item("bare"), "Doing", &key()).await.unwrap();
    assert_eq!(bare, Pushed::NoIssue);
    standard_item(&h, "i1");
    let exec = EffectRequest::new(key(), Effect::Exec { command: "true".into(), cwd: None });
    assert_eq!(push(&connectors, &h.store, &h.item("i1"), &exec).await.unwrap(), Pushed::NotSynced);
    h.store.delete_connection(TRACKER_CONNECTION).unwrap();
    let none = push_status(&connectors, &h.store, &h.item("i1"), "Doing", &key()).await.unwrap();
    assert_eq!(none, Pushed::NoConnection);
    assert!(h.store.connection(HOST_CONNECTION).unwrap().is_some());
    assert!(h.fake.writes().is_empty());
}
