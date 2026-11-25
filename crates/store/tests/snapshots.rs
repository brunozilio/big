#![allow(clippy::expect_used)]

use std::path::{Path, PathBuf};

use domain::effect::{Effect, EffectOutcome, EffectRequest, IdempotencyKey};
use domain::workflow::{ItemId, StepState, WorkItem, WorkflowEvent, replay};
use rusqlite::{Connection, params};
use serde_json::json;
use store::Store;

const EVERY: u64 = 4;

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
        events.push(WorkflowEvent::StepFinished { step, branch: None, state: StepState::Passed, output: json!(seq) });
    }
    events
}

fn rewound(id: &str) -> Vec<WorkflowEvent> {
    let mut events = story(id, 3);
    events.push(WorkflowEvent::Rewound { to: "step-1".into(), at: None });
    events.push(WorkflowEvent::StepStarted { step: "step-1".into(), branch: None });
    events
}

struct Fixture {
    _folder: tempfile::TempDir,
    path: PathBuf,
    store: Store,
}

fn fixture() -> Fixture {
    let folder = tempfile::tempdir().expect("a folder");
    let path = folder.path().join("app.db");
    let store = Store::open(&path, EVERY).expect("the store answers");
    Fixture { _folder: folder, path, store }
}

fn one_by_one(store: &Store, id: &ItemId, events: &[WorkflowEvent]) {
    for event in events {
        store.append(id, std::slice::from_ref(event)).expect("the store answers");
    }
}

fn raw(path: &Path) -> Connection {
    Connection::open(path).expect("the file opens")
}

fn break_projection(path: &Path, id: &str) {
    raw(path)
        .execute("UPDATE items SET state = '{\"shape\":\"old\"}' WHERE id = ?1", [id])
        .expect("the projection is overwritten");
}

fn break_snapshot(path: &Path, id: &str, seq: u64) {
    raw(path)
        .execute(
            "UPDATE snapshots SET state = '{\"shape\":\"old\"}' WHERE item = ?1 AND seq = ?2",
            params![id, i64::try_from(seq).expect("small")],
        )
        .expect("the snapshot is overwritten");
}

fn break_event(path: &Path, id: &str, seq: u64) {
    raw(path)
        .execute(
            "UPDATE work_events SET event = '{\"gone\":true}' WHERE item = ?1 AND seq = ?2",
            params![id, i64::try_from(seq).expect("small")],
        )
        .expect("the event is overwritten");
}

fn stored_projection(path: &Path, id: &str) -> WorkItem {
    let text: String = raw(path)
        .query_row("SELECT state FROM items WHERE id = ?1", [id], |row| row.get(0))
        .expect("the projection is there");
    serde_json::from_str(&text).expect("the projection reads in the current shape")
}

#[test]
fn a_long_item_loads_the_same_through_its_snapshot_and_through_a_full_replay() {
    let Fixture { path, store, _folder } = fixture();
    let id = ItemId::from("a");
    let events = story("a", 5);
    one_by_one(&store, &id, &events);
    assert!(events.len() as u64 > EVERY);
    assert_eq!(store.snapshot_versions(&id).expect("the store answers").last(), Some(&20));
    break_event(&path, "a", 1);
    assert_eq!(store.load(&id).expect("only the events after the snapshot are read"), Some(replay(&events)));
}

#[test]
fn an_unreadable_projection_is_rebuilt_from_the_latest_snapshot_and_kept() {
    let Fixture { path, store, _folder } = fixture();
    let id = ItemId::from("a");
    let events = story("a", 5);
    one_by_one(&store, &id, &events);
    break_projection(&path, "a");
    break_event(&path, "a", 1);
    assert_eq!(store.item(&id).expect("the store answers"), Some(replay(&events)));
    assert_eq!(stored_projection(&path, "a"), replay(&events));
}

#[test]
fn an_old_shaped_snapshot_falls_back_to_an_older_one_then_to_a_full_replay() {
    let Fixture { path, store, _folder } = fixture();
    let id = ItemId::from("a");
    let events = story("a", 5);
    one_by_one(&store, &id, &events);
    break_projection(&path, "a");
    break_snapshot(&path, "a", 20);
    assert_eq!(store.load(&id).expect("the store answers"), Some(replay(&events)));
    for seq in store.snapshot_versions(&id).expect("the store answers") {
        break_snapshot(&path, "a", seq);
    }
    assert_eq!(store.load(&id).expect("the store answers"), Some(replay(&events)));
    assert_eq!(store.item(&id).expect("the store answers"), Some(replay(&events)));
}

#[test]
fn a_rewind_after_a_snapshot_rebuilds_like_a_full_replay() {
    let Fixture { path, store, _folder } = fixture();
    let id = ItemId::from("a");
    let events = rewound("a");
    one_by_one(&store, &id, &events);
    assert!(store.snapshot_versions(&id).expect("the store answers").last() < Some(&(events.len() as u64)));
    break_projection(&path, "a");
    break_event(&path, "a", 2);
    let expected = replay(&events);
    assert_eq!(expected.steps.iter().map(|run| run.id.as_str()).collect::<Vec<_>>(), ["step-0", "step-1"]);
    assert_eq!(store.item(&id).expect("the store answers"), Some(expected.clone()));
    assert_eq!(store.load(&id).expect("the store answers"), Some(expected));
}

#[test]
fn appending_after_an_unreadable_projection_continues_from_the_rebuilt_state() {
    let Fixture { path, store, _folder } = fixture();
    let id = ItemId::from("a");
    let mut events = story("a", 3);
    one_by_one(&store, &id, &events);
    break_projection(&path, "a");
    let more = WorkflowEvent::StatusSet("later".into());
    let after = store.append(&id, std::slice::from_ref(&more)).expect("the store answers");
    events.push(more);
    assert_eq!(after, replay(&events));
    assert_eq!(store.events(&id, 0).expect("the store answers").len(), events.len());
}

#[test]
fn listings_rebuild_unreadable_projections_and_active_items_skip_ended_ones() {
    let Fixture { path, store, _folder } = fixture();
    for id in ["z", "a", "m"] {
        one_by_one(&store, &ItemId::from(id), &story(id, 2));
    }
    store.append(&ItemId::from("a"), &[WorkflowEvent::Finished { output: json!(null) }]).expect("the store answers");
    break_projection(&path, "z");
    break_projection(&path, "a");
    let ids = |items: Vec<WorkItem>| items.into_iter().map(|item| item.id.to_string()).collect::<Vec<_>>();
    assert_eq!(ids(store.active_items().expect("the store answers")), ["z", "m"]);
    assert_eq!(ids(store.items().expect("the store answers")), ["z", "a", "m"]);
    assert_eq!(stored_projection(&path, "a"), replay(&store_events(&store, "a")));
}

fn store_events(store: &Store, id: &str) -> Vec<WorkflowEvent> {
    store.events(&ItemId::from(id), 0).expect("the store answers").into_iter().map(|(_, event)| event).collect()
}

#[test]
fn reopening_the_file_reads_projections_without_replaying_any_event() {
    let Fixture { path, store, _folder } = fixture();
    let id = ItemId::from("a");
    let events = story("a", 3);
    one_by_one(&store, &id, &events);
    drop(store);
    for seq in 1..=events.len() as u64 {
        break_event(&path, "a", seq);
    }
    let reopened = Store::open(&path, EVERY).expect("the store answers");
    assert_eq!(reopened.active_items().expect("the store answers"), vec![replay(&events)]);
    assert_eq!(reopened.item(&id).expect("the store answers"), Some(replay(&events)));
}
