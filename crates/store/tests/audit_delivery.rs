#![allow(clippy::expect_used, reason = "a failed setup step stops the test")]

#[macro_use]
mod common;

use std::time::Duration;

use common::Fresh;
use domain::audit::AuditEvent;
use store::AuditDelivery;

fn events(at: i64, count: usize) -> Vec<AuditEvent> {
    (0..count).map(|_| AuditEvent::new(at, "tool.call")).collect()
}

fn seqs(store: &store::Store, sink: &str) -> Vec<u64> {
    store.audit_outbox(sink, 10).expect("outbox").1.iter().map(|record| record.seq).collect()
}

fn each_sink_reads_the_trail_from_its_own_cursor(fresh: Fresh) {
    let place = fresh();
    let store = place.open(0);
    store.append_audit(&events(1_000, 3)).expect("append");
    assert_eq!(store.audit_delivery("a").expect("a"), AuditDelivery { sink: "a".into(), ..AuditDelivery::default() });
    assert_eq!(seqs(&store, "a"), [1, 2, 3]);
    assert!(store.audit_delivered("a", 0, 2, 0).expect("advanced"));
    assert_eq!(seqs(&store, "a"), [3]);
    assert_eq!(seqs(&store, "b"), [1, 2, 3]);
    assert!(!store.audit_delivered("a", 0, 3, 0).expect("stale"), "a stale cursor does not move");
    store.audit_undelivered("a", "down").expect("failed");
    store.audit_undelivered("a", "still down").expect("failed");
    let failing = store.audit_delivery("a").expect("a");
    assert_eq!((failing.delivered, failing.attempts, failing.failure.as_deref()), (2, 2, Some("still down")));
    assert!(store.audit_delivered("a", 2, 3, 1).expect("advanced"));
    let done = store.audit_delivery("a").expect("a");
    assert_eq!((done.delivered, done.dropped, done.attempts, done.failure), (3, 1, 0, None));
    assert!(seqs(&place.open(0), "a").is_empty(), "the cursor survives a new instance");
    let listed: Vec<String> = store.audit_deliveries().expect("listed").into_iter().map(|found| found.sink).collect();
    assert_eq!(listed, ["a"]);
}

fn retention_keeps_what_an_active_sink_has_not_taken(fresh: Fresh) {
    let place = fresh();
    let store = place.open(0);
    store.append_audit(&events(1_000, 4)).expect("append");
    assert!(store.audit_delivered("a", 0, 3, 0).expect("a"));
    assert!(store.audit_delivered("b", 0, 1, 0).expect("b"));
    let sinks = ["a".to_owned(), "b".to_owned()];
    assert_eq!(store.forget_audit_delivered(Duration::from_secs(1), &sinks).expect("forget"), 1);
    assert!(store.audit_delivered("b", 1, 4, 0).expect("b"));
    assert_eq!(store.forget_audit_delivered(Duration::from_secs(1), &sinks).expect("forget"), 2);
    assert_eq!(store.forget_audit_delivered(Duration::from_secs(1), &["c".to_owned()]).expect("none"), 0);
    assert_eq!(store.verify_audit().expect("verify").broken, None);
}

fn several_instances_open_the_store_at_once(fresh: Fresh) {
    let place = fresh();
    let databases: Vec<_> = (0..6).map(|_| place.database()).collect();
    let opened: Vec<bool> = std::thread::scope(|scope| {
        let stores: Vec<_> = databases
            .into_iter()
            .map(|database| scope.spawn(move || store::Store::with_database(database, 0).is_ok()))
            .collect();
        stores.into_iter().map(|handle| handle.join().expect("joined")).collect()
    });
    assert_eq!(opened, [true; 6]);
    let store = place.open(0);
    store.append_audit(&events(1_000, 1)).expect("append");
    assert!(store.audit_delivered("a", 0, 1, 0).expect("advanced"));
}

scenarios!(
    each_sink_reads_the_trail_from_its_own_cursor,
    retention_keeps_what_an_active_sink_has_not_taken,
    several_instances_open_the_store_at_once,
);
