use std::sync::Arc;
use std::thread;

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

#[test]
fn appending_returns_and_keeps_the_projection() {
    let store = Store::in_memory(0).expect("the store answers");
    let id = ItemId::from("a");
    let events = story("a", 2);
    let item = store.append(&id, &events).expect("the store answers");
    assert_eq!(item, replay(&events));
    assert_eq!(store.item(&id).expect("the store answers"), Some(item.clone()));
    assert_eq!(store.load(&id).expect("the store answers"), Some(item));
}

#[test]
fn events_are_numbered_per_item_from_one() {
    let store = Store::in_memory(0).expect("the store answers");
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

#[test]
fn an_unknown_item_is_none() {
    let store = Store::in_memory(3).expect("the store answers");
    assert_eq!(store.load(&ItemId::from("nope")).expect("the store answers"), None);
    assert_eq!(store.item(&ItemId::from("nope")).expect("the store answers"), None);
}

#[test]
fn snapshots_are_taken_every_n_events() {
    let store = Store::in_memory(4).expect("the store answers");
    let id = ItemId::from("a");
    appended_one_by_one(&store, &id, &story("a", 2));
    assert_eq!(store.snapshot_versions(&id).expect("the store answers"), [4, 8]);
    store.append(&id, &story("a", 1)).expect("the store answers");
    assert_eq!(store.snapshot_versions(&id).expect("the store answers"), [4, 8, 17]);
}

#[test]
fn replay_with_snapshots_equals_replay_without() {
    let events = story("a", 7);
    let with = Store::in_memory(5).expect("the store answers");
    let without = Store::in_memory(0).expect("the store answers");
    let id = ItemId::from("a");
    appended_one_by_one(&with, &id, &events);
    appended_one_by_one(&without, &id, &events);
    assert!(!with.snapshot_versions(&id).expect("the store answers").is_empty());
    assert!(without.snapshot_versions(&id).expect("the store answers").is_empty());
    let from_snapshot = with.load(&id).expect("the store answers");
    assert_eq!(from_snapshot, without.load(&id).expect("the store answers"));
    assert_eq!(from_snapshot, Some(replay(&events)));
}

#[test]
fn a_rewind_is_replayed_the_same_from_a_snapshot() {
    let mut events = story("a", 4);
    events.push(WorkflowEvent::Rewound { to: "step-1".into() });
    events.push(WorkflowEvent::StepStarted { step: "step-1".into(), branch: None });
    let with = Store::in_memory(3).expect("the store answers");
    let id = ItemId::from("a");
    appended_one_by_one(&with, &id, &events);
    assert_eq!(with.load(&id).expect("the store answers"), Some(replay(&events)));
}

#[test]
fn items_are_listed_in_creation_order() {
    let store = Store::in_memory(0).expect("the store answers");
    for id in ["z", "a", "m"] {
        store.append(&ItemId::from(id), &[created(id)]).expect("the store answers");
    }
    store.append(&ItemId::from("z"), &[WorkflowEvent::StatusSet("x".into())]).expect("the store answers");
    let ids: Vec<String> =
        store.items().expect("the store answers").into_iter().map(|item| item.id.to_string()).collect();
    assert_eq!(ids, ["z", "a", "m"]);
}

#[test]
fn an_empty_append_changes_nothing() {
    let store = Store::in_memory(0).expect("the store answers");
    let id = ItemId::from("a");
    store.append(&id, &[created("a")]).expect("the store answers");
    let item = store.append(&id, &[]).expect("the store answers");
    assert_eq!(item.version, 1);
    assert_eq!(store.events(&id, 0).expect("the store answers").len(), 1);
}

#[test]
fn concurrent_appends_to_distinct_items_keep_every_log_whole() {
    let store = Arc::new(Store::in_memory(3).expect("the store answers"));
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

#[test]
fn separate_connections_to_one_file_append_concurrently() {
    let folder = tempfile::tempdir().expect("the store answers");
    let path = folder.path().join("app.db");
    Store::open(&path, 4).expect("the store answers");
    let workers: Vec<_> = (0..6)
        .map(|n| {
            let path = path.clone();
            thread::spawn(move || {
                let store = Store::open(&path, 4).expect("the store answers");
                let id = format!("item-{n}");
                appended_one_by_one(&store, &ItemId::from(id.as_str()), &story(&id, 4))
            })
        })
        .collect();
    for worker in workers {
        assert!(worker.join().expect("the worker finishes").is_some());
    }
    let store = Store::open(&path, 4).expect("the store answers");
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

#[test]
fn reopening_a_file_keeps_everything_and_migrations_run_once() {
    let folder = tempfile::tempdir().expect("the store answers");
    let path = folder.path().join("app.db");
    let events = story("a", 3);
    Store::open(&path, 2).expect("the store answers").append(&ItemId::from("a"), &events).expect("the store answers");
    let reopened = Store::open(&path, 2).expect("the store answers");
    assert_eq!(reopened.load(&ItemId::from("a")).expect("the store answers"), Some(replay(&events)));
}
