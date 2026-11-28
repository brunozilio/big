#![allow(clippy::expect_used, reason = "a failed setup step stops the test")]

#[macro_use]
mod common;

use common::Fresh;
use database::args;
use domain::workflow::{ItemId, WorkflowEvent};
use serde_json::json;
use store::SpendRow;

fn created(id: &str) -> WorkflowEvent {
    WorkflowEvent::ItemCreated { id: ItemId::from(id), workflow: "feature".into(), parent: None, input: json!({}) }
}

fn dated(store: &store::Store, id: &str, at: i64) {
    store
        .database()
        .write(|conn| conn.execute("UPDATE work_events SET at = ?1 WHERE item = ?2", args![at, id]))
        .expect("dated");
}

fn spent(store: &store::Store, at: i64, item: &str, step: &str, owner: &str, micros: u64, unpriced: u64) {
    let spent = store::Spent {
        at,
        item: item.into(),
        step: step.into(),
        owner: owner.into(),
        cost: domain::billing::Cost { known_micros: micros, unpriced_calls: unpriced, ..Default::default() },
        ..store::Spent::default()
    };
    store.record_spend(&spent).expect("spent");
}

fn row(item: &str, step: &str, owner: &str, micros: u64, unpriced: u64) -> SpendRow {
    SpendRow { item: item.into(), step: step.into(), owner: owner.into(), micros, unpriced }
}

fn items_touched_in_a_window(fresh: Fresh) {
    let place = fresh();
    let store = place.open(0);
    for (id, at) in [("i1", 10), ("i2", 50), ("i3", 99)] {
        store.append(&ItemId::from(id), &[created(id), WorkflowEvent::StatusSet("x".into())]).expect("appended");
        dated(&store, id, at);
    }
    let touched = store.touched_items(10, 99).expect("touched");
    assert_eq!(touched, [ItemId::from("i1"), ItemId::from("i2")], "the end of the window is left out");
}

fn spend_adds_up_per_item_step_and_owner_in_a_window(fresh: Fresh) {
    let place = fresh();
    let store = place.open(0);
    spent(&store, 10, "i1", "develop", "u-ana", 100, 0);
    spent(&store, 11, "i1", "develop", "u-ana", 200, 1);
    spent(&store, 12, "i1", "review", "u-ana", 50, 0);
    spent(&store, 13, "", "", "u-ana", 7, 0);
    spent(&store, 14, "i2", "plan", "u-bia", 900, 0);
    spent(&store, 30, "i1", "develop", "u-ana", 1_000, 0);
    let all = store.spend_between(10, 20, None).expect("spend");
    assert_eq!(
        all,
        [
            row("", "", "u-ana", 7, 0),
            row("i1", "develop", "u-ana", 300, 1),
            row("i1", "review", "u-ana", 50, 0),
            row("i2", "plan", "u-bia", 900, 0)
        ]
    );
    let one = store.spend_between(0, 100, Some(&ItemId::from("i1"))).expect("one item");
    assert_eq!(one, [row("i1", "develop", "u-ana", 1_300, 1), row("i1", "review", "u-ana", 50, 0)]);
}

scenarios!(
    items_touched_in_a_window,
    spend_adds_up_per_item_step_and_owner_in_a_window
);
