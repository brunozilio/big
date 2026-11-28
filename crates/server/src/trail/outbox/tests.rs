#![allow(clippy::expect_used, reason = "a failed setup step stops the test")]

use std::collections::BTreeSet;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use async_trait::async_trait;
use connectors::audit_sink::{Ack, delivered};
use connectors::provider::Credentials;
use connectors::scripted::ScriptedProvider;
use connectors::{ActionError, Http, HttpRequest, HttpResponse};
use domain::audit::AuditEvent;
use domain::connection::ConnectionId;
use serde_json::{Value, json};
use store::{AuditFilter, AuditPage, Store};

use super::*;

const DOWN_MS: u64 = 5 * 60 * 1000;
const UNTIL_MS: u64 = DOWN_MS + 60 * 1000;
const SINK: &str = "webhook_jsonl";

#[derive(Debug, Default)]
struct Destination {
    clock: AtomicU64,
    failed: AtomicUsize,
    lost_ack: AtomicBool,
    received: Mutex<Vec<String>>,
    kept: Mutex<BTreeSet<String>>,
}

impl Destination {
    fn received(&self) -> Vec<String> {
        self.received.lock().unwrap_or_else(PoisonError::into_inner).clone()
    }

    fn kept(&self) -> BTreeSet<String> {
        self.kept.lock().unwrap_or_else(PoisonError::into_inner).clone()
    }
}

#[async_trait]
impl Http for Destination {
    async fn send(&self, request: HttpRequest) -> Result<HttpResponse, ActionError> {
        if self.clock.load(Ordering::SeqCst) < DOWN_MS {
            self.failed.fetch_add(1, Ordering::SeqCst);
            return Ok(HttpResponse::new(503, "down"));
        }
        let text = String::from_utf8(request.body).expect("utf-8");
        let ids: Vec<String> = text
            .lines()
            .map(|line| {
                serde_json::from_str::<Value>(line).expect("a json line")["id"].as_str().expect("an id").to_owned()
            })
            .collect();
        self.received.lock().unwrap_or_else(PoisonError::into_inner).extend(ids.iter().cloned());
        self.kept.lock().unwrap_or_else(PoisonError::into_inner).extend(ids);
        if !self.lost_ack.swap(true, Ordering::SeqCst) {
            return Ok(HttpResponse::new(502, "the answer was lost"));
        }
        Ok(HttpResponse::new(202, ""))
    }
}

#[derive(Debug)]
struct Scripted {
    provider: ScriptedProvider,
    http: Arc<Destination>,
    creds: Credentials,
}

#[async_trait]
impl Sink for Scripted {
    fn name(&self) -> &str {
        SINK
    }

    async fn deliver(&self, events: &[Value], idempotency_key: &str) -> Result<Ack, ActionError> {
        delivered(&self.provider, self.http.as_ref(), &self.creds, events, idempotency_key).await
    }
}

fn scripted(http: &Arc<Destination>) -> Arc<dyn Sink> {
    let provider = ScriptedProvider::load_in(None, &format!("connectors/{SINK}/index.ts")).expect("the sink loads");
    let creds = Credentials::new(&ConnectionId::from(SINK), "t", json!({ "url": "https://sink.test/audit" }));
    Arc::new(Scripted { provider, http: Arc::clone(http), creds })
}

fn opened(path: &Path) -> Arc<Store> {
    Arc::new(Store::open(path, 0).expect("the store opens"))
}

fn appended(store: &Store, at: u64, count: usize) {
    let at = i64::try_from(at).expect("small");
    let events: Vec<AuditEvent> = (0..count).map(|_| AuditEvent::new(at, "tool.call")).collect();
    store.append_audit(&events).expect("appended");
}

fn hashes(store: &Store) -> BTreeSet<String> {
    let all = store.audit(&AuditFilter::default(), AuditPage::After(0), 100_000).expect("read");
    all.into_iter().map(|record| record.hash).collect()
}

async fn drained(outbox: &mut Outbox) {
    for _ in 0..100 {
        if outbox.step().await == Step::Idle {
            return;
        }
    }
    panic!("the outbox never emptied")
}

#[tokio::test]
async fn with_the_destination_down_for_five_minutes_no_event_is_lost_nor_kept_twice() {
    let folder = tempfile::tempdir().expect("a folder");
    let path = folder.path().join("app.db");
    let destination = Arc::new(Destination::default());
    let mut outbox = Outbox::new(opened(&path), scripted(&destination));
    let mut restarted = false;
    let writer = opened(&path);
    while destination.clock.load(Ordering::SeqCst) < UNTIL_MS {
        let now = destination.clock.load(Ordering::SeqCst);
        appended(&writer, now, 3);
        if !restarted && now > DOWN_MS / 2 {
            outbox = Outbox::new(opened(&path), scripted(&destination));
            restarted = true;
        }
        let advance = match outbox.step().await {
            Step::Delivered { .. } => Duration::from_millis(200),
            Step::Idle => IDLE,
            Step::Failed { wait } => wait,
        };
        destination.clock.fetch_add(u64::try_from(advance.as_millis()).expect("small"), Ordering::SeqCst);
    }
    drained(&mut outbox).await;
    let store = opened(&path);
    let written = hashes(&store);
    assert!(restarted);
    assert!(written.len() > 20, "{}", written.len());
    assert_eq!(destination.kept(), written, "every event reached the destination");
    let received = destination.received();
    assert!(received.len() > written.len(), "the batch whose answer was lost was sent again");
    assert_eq!(received.iter().collect::<BTreeSet<_>>().len(), written.len());
    let failed = destination.failed.load(Ordering::SeqCst);
    assert!((5..20).contains(&failed), "the outage was retried with backoff: {failed} attempts");
    let delivery = store.audit_delivery(SINK).expect("delivery");
    let last = store.audit(&AuditFilter::default(), AuditPage::Before(None), 1).expect("last");
    assert_eq!(Some(delivery.delivered), last.first().map(|record| record.seq));
    assert_eq!((delivery.dropped, delivery.attempts, delivery.failure), (0, 0, None));
}

#[derive(Debug)]
struct Answering(Mutex<Vec<Result<Ack, ActionError>>>);

#[async_trait]
impl Sink for Answering {
    fn name(&self) -> &str {
        "answering"
    }

    async fn deliver(&self, _: &[Value], _: &str) -> Result<Ack, ActionError> {
        let mut answers = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        if answers.is_empty() { Ok(Ack::default()) } else { answers.remove(0) }
    }
}

#[tokio::test]
async fn rejected_events_are_counted_and_a_failure_is_kept_until_the_next_delivery() {
    let folder = tempfile::tempdir().expect("a folder");
    let store = opened(&folder.path().join("app.db"));
    appended(&store, 1, 4);
    let answers = vec![
        Err(ActionError::rate_limited(Duration::from_secs(30), "slow down")),
        Ok(Ack { ack: 1, rejected: 1 }),
        Ok(Ack { ack: 5, rejected: 0 }),
        Ok(Ack { ack: 2, rejected: 0 }),
    ];
    let mut outbox = Outbox::new(Arc::clone(&store), Arc::new(Answering(Mutex::new(answers))));
    assert_eq!(outbox.step().await, Step::Failed { wait: Duration::from_secs(30) });
    let failing = store.audit_delivery("answering").expect("delivery");
    assert_eq!((failing.attempts, failing.failure.as_deref()), (1, Some("slow down")));
    assert_eq!(outbox.step().await, Step::Delivered { through: 2, acked: 1, rejected: 1 });
    assert!(matches!(outbox.step().await, Step::Failed { .. }), "an ack past what was offered is refused");
    assert_eq!(outbox.step().await, Step::Delivered { through: 4, acked: 2, rejected: 0 });
    assert_eq!(outbox.step().await, Step::Idle);
    let done = store.audit_delivery("answering").expect("delivery");
    assert_eq!((done.delivered, done.dropped, done.attempts), (4, 1, 0));
}

#[test]
fn the_wait_doubles_up_to_a_minute_and_each_event_carries_its_hash_as_id() {
    let waits: Vec<u64> = (1..=8).map(|failures| backoff(failures).as_secs()).collect();
    assert_eq!(waits, [1, 2, 4, 8, 16, 32, 60, 60]);
    let record =
        store::AuditRecord { seq: 7, event: AuditEvent::new(5, "tool.call"), prev: "p".into(), hash: "h".into() };
    let event = event_of(&record).expect("event");
    assert_eq!(
        (event["id"].clone(), event["seq"].clone(), event["action"].clone()),
        (json!("h"), json!(7), json!("tool.call"))
    );
    assert_eq!(event["schemaVersion"], json!(domain::audit::SCHEMA_VERSION));
}
