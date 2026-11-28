#![allow(clippy::expect_used, reason = "a failed setup step stops the test")]

#[macro_use]
mod common;

use std::thread;
use std::time::Duration;

use common::Fresh;

fn tick() {
    thread::sleep(Duration::from_millis(3));
}

fn entries_older_than_the_retention_and_than_the_last_cursor_move_are_pruned(fresh: Fresh) {
    let place = fresh();
    let store = place.open(0);
    store.mark_seen("c1", "old").expect("the store answers");
    tick();
    store.set_cursor("c1", "after-old").expect("the store answers");
    tick();
    store.mark_seen("c1", "fresh").expect("the store answers");
    tick();
    assert_eq!(store.prune_seen(Duration::ZERO).expect("the store answers"), 1);
    assert!(!store.was_seen("c1", "old").expect("the store answers"));
    assert!(store.was_seen("c1", "fresh").expect("the store answers"), "the cursor has not moved past it yet");
}

fn entries_inside_the_retention_are_kept_even_behind_the_cursor(fresh: Fresh) {
    let place = fresh();
    let store = place.open(0);
    store.mark_seen("c1", "e1").expect("the store answers");
    tick();
    store.set_cursor("c1", "later").expect("the store answers");
    assert_eq!(store.prune_seen(Duration::from_secs(3600)).expect("the store answers"), 0);
    assert!(store.was_seen("c1", "e1").expect("the store answers"));
}

fn a_connection_whose_cursor_is_stuck_keeps_what_it_saw_after_the_cursor_last_moved(fresh: Fresh) {
    let place = fresh();
    let store = place.open(0);
    store.set_cursor("c1", "stuck").expect("the store answers");
    tick();
    store.mark_seen("c1", "half-polled").expect("the store answers");
    tick();
    assert_eq!(store.prune_seen(Duration::ZERO).expect("the store answers"), 0);
    assert!(store.was_seen("c1", "half-polled").expect("the store answers"));
}

fn a_connection_without_a_cursor_is_pruned_by_retention_alone(fresh: Fresh) {
    let place = fresh();
    let store = place.open(0);
    store.mark_seen("quiet", "reconcile:x").expect("the store answers");
    tick();
    assert_eq!(store.prune_seen(Duration::ZERO).expect("the store answers"), 1);
    assert!(!store.was_seen("quiet", "reconcile:x").expect("the store answers"));
}

fn seeing_an_event_again_renews_it(fresh: Fresh) {
    let place = fresh();
    let store = place.open(0);
    store.mark_seen("quiet", "again").expect("the store answers");
    thread::sleep(Duration::from_millis(200));
    let cutoff = Duration::from_millis(100);
    assert!(store.was_seen("quiet", "again").expect("the store answers"));
    assert_eq!(store.prune_seen(cutoff).expect("the store answers"), 0, "it was just met again");
}

fn a_whole_batch_is_checked_and_marked_at_once_and_renews_what_it_met_again(fresh: Fresh) {
    let place = fresh();
    let store = place.open(0);
    store.mark_seen("c1", "old").expect("the store answers");
    thread::sleep(Duration::from_millis(200));
    let batch = ["old".to_owned(), "new".to_owned(), "other".to_owned()];
    let seen = store.seen_among("c1", &batch).expect("the store answers");
    assert_eq!(seen.into_iter().collect::<Vec<_>>(), ["old"]);
    assert_eq!(store.prune_seen(Duration::from_millis(100)).expect("the store answers"), 0, "renewed");
    assert_eq!(store.mark_all_seen("c1", &batch).expect("the store answers"), 2);
    assert_eq!(store.mark_all_seen("c1", &[]).expect("the store answers"), 0);
    assert!(store.seen_among("c2", &batch).expect("the store answers").is_empty(), "each connection sees its own");
    assert_eq!(store.seen_among("c1", &batch).expect("the store answers").len(), 3);
}

scenarios!(
    entries_older_than_the_retention_and_than_the_last_cursor_move_are_pruned,
    entries_inside_the_retention_are_kept_even_behind_the_cursor,
    a_connection_whose_cursor_is_stuck_keeps_what_it_saw_after_the_cursor_last_moved,
    a_connection_without_a_cursor_is_pruned_by_retention_alone,
    seeing_an_event_again_renews_it,
    a_whole_batch_is_checked_and_marked_at_once_and_renews_what_it_met_again,
);
