#![allow(clippy::expect_used, reason = "a failed setup step stops the test")]

#[macro_use]
mod common;

use std::sync::Arc;
use std::thread;

use common::Fresh;
use domain::effect::{Effect, EffectOutcome, EffectRequest, IdempotencyKey};
use domain::workflow::{ItemId, StepState, WorkItem, WorkflowEvent, replay};
use serde_json::json;
use store::Store;

fn created(id: &str) -> WorkflowEvent {
    WorkflowEvent::ItemCreated { id: ItemId::from(id), workflow: "flow".into(), parent: None, input: json!({}) }
}

fn story(id: &str, steps: usize) -> Vec<WorkflowEvent> {
    let mut events = vec![created(id)];
    for (seq, step) in (0..steps).map(|n| format!("step-{n}")).enumerate() {
        let key = IdempotencyKey::new(id, &step, None, seq as u64, 1);
        events.push(WorkflowEvent::StepStarted { step: step.clone(), branch: None });
        events.push(WorkflowEvent::EffectRequested(EffectRequest::new(
            key.clone(),
            Effect::Exec { command: "true".into(), cwd: None },
        )));
        events.push(WorkflowEvent::EffectResolved { key, outcome: EffectOutcome::Ok { value: json!(seq) } });
        events.push(WorkflowEvent::StatusSet(format!("status {seq}")));
        events.push(WorkflowEvent::StepFinished { step, branch: None, state: StepState::Passed, output: json!(seq) });
    }
    events
}

fn appended_one_by_one(store: &Store, id: &ItemId, events: &[WorkflowEvent]) -> Option<WorkItem> {
    events.iter().map(|event| store.append(id, std::slice::from_ref(event)).ok()).last().flatten()
}

fn appending_returns_and_keeps_the_projection(fresh: Fresh) {
    let place = fresh();
    let store = place.open(0);
    let id = ItemId::from("a");
    let events = story("a", 2);
    let item = store.append(&id, &events).expect("the store answers");
    assert_eq!(item, replay(&events));
    assert_eq!(store.item(&id).expect("the store answers"), Some(item.clone()));
    assert_eq!(store.load(&id).expect("the store answers"), Some(item));
}

fn events_are_numbered_per_item_from_one(fresh: Fresh) {
    let place = fresh();
    let store = place.open(0);
    store.append(&ItemId::from("a"), &story("a", 1)).expect("the store answers");
    store.append(&ItemId::from("b"), &[created("b")]).expect("the store answers");
    store.append(&ItemId::from("a"), &[WorkflowEvent::StatusSet("x".into())]).expect("the store answers");
    let seqs: Vec<u64> =
        store.events(&ItemId::from("a"), 0).expect("the store answers").into_iter().map(|(seq, _)| seq).collect();
    assert_eq!(seqs, (1..=7).collect::<Vec<_>>());
    let after: Vec<u64> =
        store.events(&ItemId::from("a"), 5).expect("the store answers").into_iter().map(|(seq, _)| seq).collect();
    assert_eq!(after, [6, 7]);
    assert_eq!(store.events(&ItemId::from("b"), 0).expect("the store answers").len(), 1);
}

fn an_unknown_item_is_none(fresh: Fresh) {
    let place = fresh();
    let store = place.open(3);
    assert_eq!(store.load(&ItemId::from("nope")).expect("the store answers"), None);
    assert_eq!(store.item(&ItemId::from("nope")).expect("the store answers"), None);
}

fn snapshots_are_taken_every_n_events(fresh: Fresh) {
    let place = fresh();
    let store = place.open(4);
    let id = ItemId::from("a");
    appended_one_by_one(&store, &id, &story("a", 2));
    assert_eq!(store.snapshot_versions(&id).expect("the store answers"), [4, 8]);
    store.append(&id, &story("a", 1)).expect("the store answers");
    assert_eq!(store.snapshot_versions(&id).expect("the store answers"), [4, 8, 17]);
}

fn replay_with_snapshots_equals_replay_without(fresh: Fresh) {
    let events = story("a", 7);
    let (here, there) = (fresh(), fresh());
    let with = here.open(5);
    let without = there.open(0);
    let id = ItemId::from("a");
    appended_one_by_one(&with, &id, &events);
    appended_one_by_one(&without, &id, &events);
    assert!(!with.snapshot_versions(&id).expect("the store answers").is_empty());
    assert!(without.snapshot_versions(&id).expect("the store answers").is_empty());
    let from_snapshot = with.load(&id).expect("the store answers");
    assert_eq!(from_snapshot, without.load(&id).expect("the store answers"));
    assert_eq!(from_snapshot, Some(replay(&events)));
}

fn a_rewind_is_replayed_the_same_from_a_snapshot(fresh: Fresh) {
    let mut events = story("a", 4);
    events.push(WorkflowEvent::Rewound { to: "step-1".into(), at: None });
    events.push(WorkflowEvent::StepStarted { step: "step-1".into(), branch: None });
    let place = fresh();
    let with = place.open(3);
    let id = ItemId::from("a");
    appended_one_by_one(&with, &id, &events);
    assert_eq!(with.load(&id).expect("the store answers"), Some(replay(&events)));
}

fn items_are_listed_in_creation_order(fresh: Fresh) {
    let place = fresh();
    let store = place.open(0);
    for id in ["z", "a", "m"] {
        store.append(&ItemId::from(id), &[created(id)]).expect("the store answers");
    }
    store.append(&ItemId::from("z"), &[WorkflowEvent::StatusSet("x".into())]).expect("the store answers");
    let ids: Vec<String> =
        store.items().expect("the store answers").into_iter().map(|item| item.id.to_string()).collect();
    assert_eq!(ids, ["z", "a", "m"]);
}

fn an_empty_append_changes_nothing(fresh: Fresh) {
    let place = fresh();
    let store = place.open(0);
    let id = ItemId::from("a");
    store.append(&id, &[created("a")]).expect("the store answers");
    let item = store.append(&id, &[]).expect("the store answers");
    assert_eq!(item.version, 1);
    assert_eq!(store.events(&id, 0).expect("the store answers").len(), 1);
}

fn concurrent_appends_to_distinct_items_keep_every_log_whole(fresh: Fresh) {
    let place = fresh();
    let store = Arc::new(place.open(3));
    let workers: Vec<_> = (0..8)
        .map(|n| {
            let store = Arc::clone(&store);
            thread::spawn(move || {
                let id = format!("item-{n}");
                appended_one_by_one(&store, &ItemId::from(id.as_str()), &story(&id, 5))
            })
        })
        .collect();
    for (n, worker) in workers.into_iter().enumerate() {
        let id = format!("item-{n}");
        let expected = replay(&story(&id, 5));
        assert_eq!(worker.join().expect("the worker finishes"), Some(expected.clone()));
        assert_eq!(store.load(&ItemId::from(id.as_str())).expect("the store answers"), Some(expected));
    }
    assert_eq!(store.items().expect("the store answers").len(), 8);
}

fn separate_connections_to_one_file_append_concurrently(fresh: Fresh) {
    let place = fresh();
    place.open(4);
    thread::scope(|scope| {
        let workers: Vec<_> = (0..6)
            .map(|n| {
                let place = &place;
                scope.spawn(move || {
                    let store = place.open(4);
                    let id = format!("item-{n}");
                    appended_one_by_one(&store, &ItemId::from(id.as_str()), &story(&id, 4))
                })
            })
            .collect();
        for worker in workers {
            assert!(worker.join().expect("the worker finishes").is_some());
        }
    });
    let store = place.open(4);
    for n in 0..6 {
        let id = format!("item-{n}");
        let events: Vec<WorkflowEvent> = store
            .events(&ItemId::from(id.as_str()), 0)
            .expect("the store answers")
            .into_iter()
            .map(|(_, event)| event)
            .collect();
        assert_eq!(events, story(&id, 4));
        assert_eq!(store.load(&ItemId::from(id.as_str())).expect("the store answers"), Some(replay(&events)));
    }
}

fn reopening_a_file_keeps_everything_and_migrations_run_once(fresh: Fresh) {
    let place = fresh();
    let events = story("a", 3);
    place.open(2).append(&ItemId::from("a"), &events).expect("the store answers");
    let reopened = place.open(2);
    assert_eq!(reopened.load(&ItemId::from("a")).expect("the store answers"), Some(replay(&events)));
}

fn timed_events_carry_the_instant_they_were_appended(fresh: Fresh) {
    let place = fresh();
    let store = place.open(0);
    let id = ItemId::from("a");
    store.append(&id, &story("a", 1)).expect("the store answers");
    let timed = store.timed_events(&id, 2).expect("the store answers");
    let plain = store.events(&id, 2).expect("the store answers");
    assert_eq!(timed.len(), plain.len());
    assert!(timed.iter().zip(&plain).all(|((seq, at, event), (other, same))| seq == other && event == same && *at > 0));
}

#[test]
fn a_file_from_before_the_creation_order_column_keeps_its_order() {
    let place = common::Place::sqlite();
    let store = place.open(0);
    for id in ["z", "a", "m"] {
        store.append(&ItemId::from(id), &[created(id)]).expect("the store answers");
    }
    drop(store);
    place.raw("UPDATE items SET seq = 0", &[]);
    let reopened = place.open(0);
    reopened.append(&ItemId::from("b"), &[created("b")]).expect("the store answers");
    let ids: Vec<String> =
        reopened.items().expect("the store answers").into_iter().map(|item| item.id.to_string()).collect();
    assert_eq!(ids, ["z", "a", "m", "b"]);
}

scenarios!(
    appending_returns_and_keeps_the_projection,
    events_are_numbered_per_item_from_one,
    an_unknown_item_is_none,
    snapshots_are_taken_every_n_events,
    replay_with_snapshots_equals_replay_without,
    a_rewind_is_replayed_the_same_from_a_snapshot,
    items_are_listed_in_creation_order,
    an_empty_append_changes_nothing,
    concurrent_appends_to_distinct_items_keep_every_log_whole,
    separate_connections_to_one_file_append_concurrently,
    reopening_a_file_keeps_everything_and_migrations_run_once,
    timed_events_carry_the_instant_they_were_appended,
);
