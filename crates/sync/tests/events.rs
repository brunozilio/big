#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use std::sync::Arc;
use std::time::Duration;

use connectors::{ActionError, TokenStore};
use domain::connection::ConnectionId;
use domain::workflow::WorkflowEvent;
use serde_json::json;
use store::Store;
use support::{
    Fake, HOST_CONNECTION, TRACKER_CONNECTION, config, event, harness, harness_on, harness_with, standard_item, wait,
};
use sync::{ConnectionState, SyncConfig};
use tokio::time::Instant;

const STATUS: &str = "tracker.status.changed";

fn status(id: &str, to: &str, second: i64) -> connectors::ExternalEvent {
    event(id, TRACKER_CONNECTION, STATUS, "K-1", json!({"to": to}), second)
}

#[tokio::test]
async fn a_duplicated_event_is_delivered_once_within_and_across_polls() {
    let h = harness().await;
    standard_item(&h, "i1");
    h.sink.wait_on("K-1");
    h.queue(Ok(vec![status("e1", "Done", 10), status("e1", "Done", 10)]));
    h.queue(Ok(vec![status("e1", "Done", 10), status("e2", "Doing", 11)]));
    let first = h.sync.poll_now(TRACKER_CONNECTION).await.unwrap().unwrap();
    let second = h.sync.poll_now(TRACKER_CONNECTION).await.unwrap().unwrap();
    assert_eq!((first.fetched, first.duplicates, first.delivered), (2, 1, 1));
    assert_eq!((second.fetched, second.duplicates, second.delivered), (2, 1, 1));
    let ids: Vec<String> = h.sink.delivered().into_iter().map(|e| e.id).collect();
    assert_eq!(ids, ["e1", "e2"]);
}

#[tokio::test]
async fn events_out_of_order_are_routed_by_time_and_the_projection_keeps_the_newest() {
    let h = harness().await;
    standard_item(&h, "i1");
    h.queue(Ok(vec![status("late", "Done", 20), status("early", "Review", 10)]));
    h.queue(Ok(vec![status("stale", "Todo", 5)]));
    let report = h.sync.poll_now(TRACKER_CONNECTION).await.unwrap().unwrap();
    assert_eq!((report.delivered, report.observed), (0, 2));
    h.sync.poll_now(TRACKER_CONNECTION).await.unwrap();
    let order: Vec<String> = h
        .sink
        .recorded()
        .into_iter()
        .filter_map(|(_, e)| match e {
            WorkflowEvent::ExternalObserved(seen) => Some(seen.id),
            _ => None,
        })
        .collect();
    assert_eq!(order, ["early", "late", "stale"]);
    assert_eq!(h.item("i1").latest_observed("K-1").map(|seen| seen.id.clone()), Some("late".into()));
}

#[tokio::test]
async fn an_event_nobody_waits_for_fires_triggers_and_only_touches_items_of_its_subject() {
    let h = harness().await;
    standard_item(&h, "i1");
    h.sink.trigger_on("code.pr.merged");
    let merged = event("m", HOST_CONNECTION, "code.pr.merged", "o/r#7", json!({}), 30);
    let other = event("x", HOST_CONNECTION, "code.pr.merged", "o/r#99", json!({}), 31);
    h.queue(Ok(vec![merged, other]));
    let report = h.sync.poll_now(HOST_CONNECTION).await.unwrap().unwrap();
    assert_eq!((report.delivered, report.triggered, report.observed), (0, 2, 1));
    assert_eq!(h.item("i1").observed.len(), 1);
}

#[tokio::test]
async fn the_cursor_survives_a_restart() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("app.db");
    let fake = Fake::leaked();
    {
        let h = harness_on(Arc::new(Store::open(&path, 0).unwrap()), fake).await;
        h.sync.poll_now(TRACKER_CONNECTION).await.unwrap();
        h.sync.poll_now(TRACKER_CONNECTION).await.unwrap();
    }
    let h = harness_on(Arc::new(Store::open(&path, 0).unwrap()), fake).await;
    h.queue(Ok(vec![status("e1", "Done", 10)]));
    h.sync.poll_now(TRACKER_CONNECTION).await.unwrap();
    let cursors = fake.world().cursors.clone();
    assert_eq!(cursors, [None, Some("1".into()), Some("2".into())]);
    assert_eq!(h.store.cursor(TRACKER_CONNECTION).unwrap().as_deref(), Some("3"));
    assert!(h.store.was_seen(TRACKER_CONNECTION, "e1").unwrap());
}

#[tokio::test]
async fn a_failed_poll_keeps_the_cursor_so_the_batch_is_read_again() {
    let h = harness().await;
    h.store.set_cursor(TRACKER_CONNECTION, "7").unwrap();
    h.store.delete_connection(HOST_CONNECTION).unwrap();
    h.store
        .save_connection(&store::ConnectionRecord {
            id: TRACKER_CONNECTION.into(),
            provider: "missing".into(),
            label: "x".into(),
            account: None,
            capabilities: vec!["tracker".into()],
            settings: json!({}),
        })
        .unwrap();
    assert!(h.sync.poll_now(TRACKER_CONNECTION).await.is_err());
    assert_eq!(h.store.cursor(TRACKER_CONNECTION).unwrap().as_deref(), Some("7"));
}

#[tokio::test(start_paused = true)]
async fn polling_is_fast_only_while_an_item_waits_on_the_connection() {
    let h = harness().await;
    standard_item(&h, "i1");
    let start = Instant::now();
    h.sync.tick().await.unwrap();
    let host = ConnectionId::from(HOST_CONNECTION);
    let tracker = ConnectionId::from(TRACKER_CONNECTION);
    assert_eq!(h.sync.due(&host), Some(start + Duration::from_secs(100)));
    h.add("i1", &[wait("i1", 2, "code.review.submitted", json!({}))]);
    h.sync.hurry();
    h.sync.tick().await.unwrap();
    assert_eq!(h.sync.due(&host), Some(start + Duration::from_secs(10)));
    assert_eq!(h.sync.due(&tracker), Some(start + Duration::from_secs(100)));
}

#[tokio::test(start_paused = true)]
async fn a_rate_limited_connection_waits_what_the_server_asked() {
    let h = harness().await;
    h.queue(Ok(Vec::new()));
    h.fake.world().fail = Some(ActionError::rate_limited(Duration::from_secs(30), "slow down"));
    let start = Instant::now();
    let (report, _) = h.sync.tick().await.unwrap();
    assert!(report.polled.iter().all(|(_, outcome)| outcome.is_err()));
    let tracker = ConnectionId::from(TRACKER_CONNECTION);
    assert!(h.sync.due(&tracker).is_some_and(|due| due >= start + Duration::from_secs(30)));
    h.fake.world().fail = None;
    let polls = h.fake.world().cursors.len();
    tokio::time::advance(Duration::from_secs(10)).await;
    h.sync.tick().await.unwrap();
    assert_eq!(h.fake.world().cursors.len(), polls);
    tokio::time::advance(Duration::from_secs(21)).await;
    h.sync.tick().await.unwrap();
    assert_eq!(h.fake.world().cursors.len(), polls + 2);
}

#[tokio::test(start_paused = true)]
async fn transient_failures_back_off_and_success_resets_the_connection() {
    let h = harness().await;
    h.fake.world().fail = Some(ActionError::transient("timeout"));
    let tracker = ConnectionId::from(TRACKER_CONNECTION);
    let start = Instant::now();
    h.sync.tick().await.unwrap();
    assert_eq!(h.sync.due(&tracker), Some(start + Duration::from_secs(2)));
    tokio::time::advance(Duration::from_secs(2)).await;
    h.sync.tick().await.unwrap();
    assert_eq!(h.sync.due(&tracker), Some(start + Duration::from_secs(2 + 4)));
    assert!(matches!(h.sync.health().state(&tracker), ConnectionState::Failing { attempt: 2, .. }));
    h.fake.world().fail = None;
    tokio::time::advance(Duration::from_secs(4)).await;
    h.sync.tick().await.unwrap();
    assert_eq!(h.sync.health().state(&tracker), ConnectionState::Healthy);
}

#[tokio::test(start_paused = true)]
async fn a_revoked_connection_needs_reconnecting_once_and_recovers_after() {
    let h = harness().await;
    standard_item(&h, "i1");
    h.tokens.remove(&ConnectionId::from(TRACKER_CONNECTION)).await.unwrap();
    let tracker = ConnectionId::from(TRACKER_CONNECTION);
    h.sync.tick().await.unwrap();
    assert!(h.sync.health().needs_reconnect(&tracker));
    assert!(!h.sync.health().needs_reconnect(&ConnectionId::from(HOST_CONNECTION)));
    h.sync.reconcile_now().await.unwrap();
    let flagged = h.sink.reconciled().into_iter().filter(|(field, _)| field == "connection").count();
    assert_eq!(flagged, 1);
    h.tokens.save(&tracker, &connectors::Grant::bearer("new")).await.unwrap();
    h.sync.hurry();
    h.sync.tick().await.unwrap();
    assert_eq!(h.sync.health().state(&tracker), ConnectionState::Healthy);
}

#[tokio::test(start_paused = true)]
async fn the_loop_stops_when_asked() {
    let h = Arc::new(harness().await);
    let runner = h.clone();
    let task = tokio::spawn(async move { runner.sync.run().await });
    tokio::time::advance(Duration::from_secs(1)).await;
    h.sync.stop();
    tokio::time::timeout(Duration::from_secs(5), task).await.unwrap().unwrap();
    assert!(!h.fake.world().cursors.is_empty());
}

#[tokio::test]
async fn a_sink_failure_mid_batch_keeps_the_cursor_and_delivers_the_rest_once_later() {
    let h = harness().await;
    standard_item(&h, "i1");
    h.sink.wait_on("K-1");
    let batch = vec![status("a", "Review", 10), status("b", "Done", 20)];
    h.queue(Ok(batch.clone()));
    h.queue(Ok(batch));
    *h.sink.refuse.lock().unwrap() = Some("b".into());
    assert!(h.sync.poll_now(TRACKER_CONNECTION).await.is_err());
    assert_eq!(h.store.cursor(TRACKER_CONNECTION).unwrap(), None);
    *h.sink.refuse.lock().unwrap() = None;
    let report = h.sync.poll_now(TRACKER_CONNECTION).await.unwrap().unwrap();
    assert_eq!((report.duplicates, report.delivered), (1, 1));
    let ids: Vec<String> = h.sink.delivered().into_iter().map(|e| e.id).collect();
    assert_eq!(ids, ["a", "b"]);
    assert_eq!(h.store.cursor(TRACKER_CONNECTION).unwrap().as_deref(), Some("1"));
}

#[tokio::test]
async fn the_reconcile_pass_forgets_seen_events_past_the_retention() {
    let short = SyncConfig { seen_retention: Duration::ZERO, ..config() };
    let h = harness_with(Arc::new(Store::in_memory(0).unwrap()), Fake::leaked(), short).await;
    h.store.mark_seen(TRACKER_CONNECTION, "old").unwrap();
    std::thread::sleep(Duration::from_millis(3));
    h.sync.tick().await.unwrap();
    assert!(!h.store.was_seen(TRACKER_CONNECTION, "old").unwrap());
}

#[tokio::test]
async fn seen_events_inside_the_retention_survive_the_reconcile_pass() {
    let h = harness().await;
    h.store.mark_seen(TRACKER_CONNECTION, "recent").unwrap();
    std::thread::sleep(Duration::from_millis(3));
    h.sync.tick().await.unwrap();
    assert!(h.store.was_seen(TRACKER_CONNECTION, "recent").unwrap());
}

#[tokio::test(start_paused = true)]
async fn a_permanent_failure_is_not_retried_until_the_connection_changes() {
    let h = harness().await;
    let tracker = ConnectionId::from(TRACKER_CONNECTION);
    h.fake.world().fail = Some(ActionError::permanent("the connection has no project to watch"));
    h.sync.tick().await.unwrap();
    let polled = h.fake.world().cursors.len();
    assert!(h.sync.is_parked(&tracker));

    tokio::time::advance(Duration::from_secs(1_000)).await;
    h.sync.hurry();
    let (_, next) = h.sync.tick().await.unwrap();
    assert_eq!(h.fake.world().cursors.len(), polled, "nothing is polled again");
    assert!(next > Instant::now(), "a parked connection does not keep the loop awake");

    h.fake.world().fail = None;
    h.store
        .save_connection(&store::ConnectionRecord {
            id: TRACKER_CONNECTION.into(),
            provider: "fake".into(),
            label: TRACKER_CONNECTION.into(),
            account: None,
            capabilities: vec!["tracker".into()],
            settings: json!({"project": "K"}),
        })
        .unwrap();
    h.sync.tick().await.unwrap();
    assert_eq!(h.fake.world().cursors.len(), polled + 1, "only the changed connection is polled again");
    assert!(!h.sync.is_parked(&tracker));
    assert!(h.sync.is_parked(&ConnectionId::from(HOST_CONNECTION)));
}
