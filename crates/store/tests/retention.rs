#![allow(clippy::expect_used, reason = "a failed setup step stops the test")]

#[macro_use]
mod common;

use std::time::Duration;

use common::Fresh;
use domain::workflow::{ItemId, WorkflowEvent};
use serde_json::{Value, json};
use store::{Forgotten, Store};

fn created(id: &str, input: Value) -> WorkflowEvent {
    WorkflowEvent::ItemCreated { id: ItemId::from(id), workflow: "flow".into(), parent: None, input }
}

fn lived(store: &Store, id: &str, input: Value, ended: bool) -> ItemId {
    let item = ItemId::from(id);
    let mut events = vec![created(id, input)];
    events.extend((0..5).map(|n| WorkflowEvent::StatusSet(format!("status {n}"))));
    if ended {
        events.push(WorkflowEvent::Cancelled { reason: "done with it".into() });
    }
    for event in &events {
        store.append(&item, std::slice::from_ref(event)).expect("append");
    }
    item
}

fn without_a_retention_only_the_older_snapshots_of_ended_items_go(fresh: Fresh) {
    let place = fresh();
    let store = place.open(2);
    let ended = lived(&store, "ended", json!({}), true);
    let active = lived(&store, "active", json!({}), false);
    let before = store.item(&ended).expect("item");
    assert!(store.snapshot_versions(&ended).expect("snapshots").len() > 1);

    let forgotten = store.forget_ended(None).expect("forget");

    assert_eq!(forgotten.items, 0);
    assert!(forgotten.snapshots > 0);
    assert_eq!(store.snapshot_versions(&ended).expect("snapshots").len(), 1);
    assert_eq!(store.snapshot_versions(&active).expect("snapshots").len(), 3, "an active item keeps them all");
    assert_eq!(store.item(&ended).expect("item"), before);
    assert_eq!(store.load(&ended).expect("rebuilt"), before, "the item still rebuilds from what is left");
}

fn an_item_ended_long_ago_is_forgotten_unless_an_active_item_mentions_it(fresh: Fresh) {
    let place = fresh();
    let store = place.open(2);
    let old = lived(&store, "old", json!({}), true);
    let linked = lived(&store, "linked", json!({}), true);
    let active = lived(&store, "active", json!({ "story": "linked" }), false);
    std::thread::sleep(Duration::from_millis(5));

    assert_eq!(store.forget_ended(Some(Duration::from_secs(3600))).expect("forget").items, 0, "nothing is old yet");
    let forgotten = store.forget_ended(Some(Duration::ZERO)).expect("forget");

    assert_eq!(forgotten, Forgotten { items: 1, snapshots: 0 });
    assert_eq!(store.item(&old).expect("item"), None);
    assert!(store.events(&old, 0).expect("events").is_empty());
    assert!(store.item(&linked).expect("item").is_some(), "an active item still points at it");
    assert!(store.item(&active).expect("item").is_some());
    assert_eq!(store.events(&active, 0).expect("events").len(), 6);
}

fn spent(at: i64) -> store::Spent {
    store::Spent {
        at,
        item: "i1".into(),
        step: "develop".into(),
        agent: "dev".into(),
        application: String::new(),
        owner: "u-ana".into(),
        origin: domain::origin::Origin::Step,
        cost: domain::billing::Cost { known_micros: 100, ..Default::default() },
    }
}

fn call(at: i64) -> store::ConnectorCall<'static> {
    store::ConnectorCall { at, connection: "c1", action: "a.b", origin: None, item: None, step: None, ok: true }
}

fn old_connector_calls_go_and_spend_stays_while_its_month_still_counts(fresh: Fresh) {
    use domain::quota::Scope;
    let place = fresh();
    let store = place.open(2);
    let now = jiff::Timestamp::now().as_millisecond();
    let today = jiff::Zoned::now().date().first_of_month().to_zoned(jiff::tz::TimeZone::system());
    let month = today.expect("month start").timestamp().as_millisecond();
    for at in [1, now - 1] {
        store.count_connector_call(&call(at)).expect("counted");
    }
    for at in [1, month, now - 1] {
        store.record_spend(&spent(at)).expect("kept");
    }

    store.forget_ended(None).expect("forget");
    assert_eq!(store.connector_calls_since(0).expect("calls").iter().map(|row| row.calls).sum::<u64>(), 2);

    store.forget_ended(Some(Duration::ZERO)).expect("forget");
    assert!(store.connector_calls_since(0).expect("calls").is_empty(), "every call is older than a zero cutoff");
    let kept = store.spent_since(0, Scope::Person, "u-ana").expect("spend");
    assert_eq!(kept.known_micros, 200, "the spend of this month stays for the ceilings");
}

scenarios!(
    without_a_retention_only_the_older_snapshots_of_ended_items_go,
    an_item_ended_long_ago_is_forgotten_unless_an_active_item_mentions_it,
    old_connector_calls_go_and_spend_stays_while_its_month_still_counts,
);
