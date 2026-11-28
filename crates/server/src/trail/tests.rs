use std::sync::Arc;

use serde_json::json;
use store::{AuditFilter, AuditPage};
use tracing::Level;
use tracing_subscriber::layer::SubscriberExt;

use super::*;

fn captured_with(capacity: usize, emit: impl FnOnce()) -> (Arc<Queue>, Vec<Captured>) {
    let queue = Arc::new(Queue::new(capacity));
    let skip = vec!["http.server.request".to_owned()];
    let subscriber =
        tracing_subscriber::registry().with(Capture::new(Arc::clone(&queue), Level::INFO, skip).filtered());
    tracing::subscriber::with_default(subscriber, emit);
    let receiver = queue.take_receiver().expect("receiver");
    (Arc::clone(&queue), receiver.try_iter().collect())
}

#[test]
fn audit_events_are_captured_with_the_agent_and_person_around_them() {
    let (_, got) = captured_with(10, || {
        let run = tracing::info_span!(target: "audit", "agent.run", agent = "dev", user = "u-ana");
        run.in_scope(|| {
            tracing::event!(target: "audit", Level::WARN, gen_ai.tool.name = "bash", outcome = "failure", "tool.call");
            tracing::debug!(target: "audit", "tool.result");
            tracing::info!(target: "audit", "http.server.request");
            tracing::info!(target: "other", "not.audit");
        });
        tracing::info!(target: "audit", user = "u-bia", "auth.signed_in");
    });
    assert_eq!(
        got.iter().map(|captured| captured.action.as_str()).collect::<Vec<_>>(),
        ["tool.call", "auth.signed_in"]
    );
    assert_eq!(got[0].fields["agent"], "dev");
    assert_eq!(got[0].fields["user"], "u-ana");
    assert_eq!(got[0].fields["gen_ai.tool.name"], "bash");
    assert_eq!(got[1].fields["user"], "u-bia");
    assert!(!got[1].fields.contains_key("agent"));
}

#[test]
fn a_full_queue_drops_and_counts_instead_of_waiting() {
    let (queue, got) = captured_with(2, || {
        for _ in 0..5 {
            tracing::info!(target: "audit", "item.killed");
        }
    });
    assert_eq!(got.len(), 2);
    assert_eq!(queue.take_dropped(), 3);
}

#[test]
fn the_writer_keeps_the_captured_events_and_tells_what_was_dropped() {
    let store = Arc::new(Store::in_memory(0).expect("store"));
    let (queue, got) = captured_with(1, || {
        tracing::warn!(target: "audit", agent = "dev", layer = "floor", "policy.blocked");
        tracing::warn!(target: "audit", agent = "dev", "policy.blocked");
    });
    let writer =
        Writer { store: Arc::clone(&store), redactor: Arc::default(), queue, retention: None, sinks: Vec::new() };
    let mut pending = got.into_iter().map(|captured| entry::draft_of(captured, &writer.redactor)).collect();
    writer.write(&mut pending);
    assert!(pending.is_empty());
    let kept = store.audit(&AuditFilter::default(), AuditPage::After(0), 10).expect("kept");
    assert_eq!(
        kept.iter().map(|record| record.event.action.as_str()).collect::<Vec<_>>(),
        ["policy.blocked", "audit.dropped"]
    );
    assert_eq!(kept[0].event.actor.as_deref(), Some("dev"));
    assert_eq!(kept[0].event.policy.as_deref(), Some("floor"));
    assert_eq!(kept[1].event.detail, json!({ "count": 1 }));
    assert_eq!(store.verify_audit().expect("verify").broken, None);
}

#[test]
fn the_settings_keep_the_trail_unless_a_retention_is_declared() {
    let kept = Settings::read("", "a b,c", "x", "");
    assert_eq!((kept.level, kept.capacity, kept.retention), (Level::INFO, 10_000, None));
    assert_eq!(kept.skip, ["a", "b", "c"]);
    let forgets = Settings::read("warn", "", "5", "30");
    assert_eq!((forgets.level, forgets.capacity), (Level::WARN, 5));
    assert_eq!(forgets.retention, Some(Duration::from_secs(30 * DAY)));
    assert!(forgets.sinks.is_empty());
    assert_eq!(forgets.with_sinks("one, two").sinks, ["one", "two"]);
}
