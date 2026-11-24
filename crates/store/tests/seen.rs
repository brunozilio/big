use std::thread;
use std::time::Duration;

use store::Store;

fn tick() {
    thread::sleep(Duration::from_millis(3));
}

#[test]
fn entries_older_than_the_retention_and_than_the_last_cursor_move_are_pruned() {
    let store = Store::in_memory(0).expect("the store answers");
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

#[test]
fn entries_inside_the_retention_are_kept_even_behind_the_cursor() {
    let store = Store::in_memory(0).expect("the store answers");
    store.mark_seen("c1", "e1").expect("the store answers");
    tick();
    store.set_cursor("c1", "later").expect("the store answers");
    assert_eq!(store.prune_seen(Duration::from_secs(3600)).expect("the store answers"), 0);
    assert!(store.was_seen("c1", "e1").expect("the store answers"));
}

#[test]
fn a_connection_whose_cursor_is_stuck_keeps_what_it_saw_after_the_cursor_last_moved() {
    let store = Store::in_memory(0).expect("the store answers");
    store.set_cursor("c1", "stuck").expect("the store answers");
    tick();
    store.mark_seen("c1", "half-polled").expect("the store answers");
    tick();
    assert_eq!(store.prune_seen(Duration::ZERO).expect("the store answers"), 0);
    assert!(store.was_seen("c1", "half-polled").expect("the store answers"));
}

#[test]
fn a_connection_without_a_cursor_is_pruned_by_retention_alone() {
    let store = Store::in_memory(0).expect("the store answers");
    store.mark_seen("quiet", "reconcile:x").expect("the store answers");
    tick();
    assert_eq!(store.prune_seen(Duration::ZERO).expect("the store answers"), 1);
    assert!(!store.was_seen("quiet", "reconcile:x").expect("the store answers"));
}

#[test]
fn seeing_an_event_again_renews_it() {
    let store = Store::in_memory(0).expect("the store answers");
    store.mark_seen("quiet", "again").expect("the store answers");
    thread::sleep(Duration::from_millis(200));
    let cutoff = Duration::from_millis(100);
    assert!(store.was_seen("quiet", "again").expect("the store answers"));
    assert_eq!(store.prune_seen(cutoff).expect("the store answers"), 0, "it was just met again");
}
