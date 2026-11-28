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
const SINK: &str = "fixture_sink";

const FIXTURE: &str = r#"import { fetch } from "sdk/connectors/index.ts";
import { audit } from "sdk/connectors/contracts.ts";
export function provider() {
  return { schema_version: 2, name: "Fixture sink", capabilities: ["audit"], auth: { kind: "token", fields: [] }, actions: audit() };
}
export function call({ args, creds }, ctx) {
  const sent = (args.events ?? []).slice(0, 100);
  if (sent.length === 0) return { ack: 0 };
  const body = sent.map((event) => JSON.stringify(event)).join("\n");
  fetch(ctx, { method: "POST", url: creds.settings.url, headers: { "content-type": "application/x-ndjson" }, body });
  return { ack: sent.length };
}
"#;

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

fn fixture() -> ScriptedProvider {
    let folder = tempfile::tempdir().expect("a folder");
    let dir = folder.path().join(format!("connectors/{SINK}"));
    std::fs::create_dir_all(&dir).expect("dir");
    std::fs::write(dir.join("index.ts"), FIXTURE).expect("fixture");
    ScriptedProvider::load_in(Some(folder.path()), &format!("connectors/{SINK}/index.ts")).expect("the sink loads")
}

fn pace() -> Pace {
    Pace::configured().expect("the [audit] pace reads")
}

fn scripted(http: &Arc<Destination>) -> Arc<dyn Sink> {
    let creds = Credentials::new(&ConnectionId::from(SINK), "t", json!({ "url": "https://sink.test/audit" }));
    Arc::new(Scripted { provider: fixture(), http: Arc::clone(http), creds })
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
    let mut outbox = Outbox::new(opened(&path), scripted(&destination), pace());
    let mut restarted = false;
    let writer = opened(&path);
    while destination.clock.load(Ordering::SeqCst) < UNTIL_MS {
        let now = destination.clock.load(Ordering::SeqCst);
        appended(&writer, now, 3);
        if !restarted && now > DOWN_MS / 2 {
            outbox = Outbox::new(opened(&path), scripted(&destination), pace());
            restarted = true;
        }
        let advance = match outbox.step().await {
            Step::Delivered { .. } => Duration::from_millis(200),
            Step::Idle => pace().idle,
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
    let polled = usize::try_from(DOWN_MS / u64::try_from(pace().first_wait.as_millis()).expect("small")).expect("small");
    assert!(failed >= 3 && failed * 4 < polled, "the outage was retried with backoff: {failed} attempts");
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
        Err(ActionError::rate_limited(pace().most_wait * 2, "slow down")),
        Ok(Ack { ack: 1, rejected: 1 }),
        Ok(Ack { ack: 5, rejected: 0 }),
        Ok(Ack { ack: 2, rejected: 0 }),
    ];
    let mut outbox = Outbox::new(Arc::clone(&store), Arc::new(Answering(Mutex::new(answers))), pace());
    assert_eq!(outbox.step().await, Step::Failed { wait: pace().most_wait });
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
fn a_sink_without_a_connector_stops_the_boot_and_no_sink_changes_nothing() {
    let http: Arc<dyn Http> = Arc::new(connectors::FakeHttp::new());
    let tokens = Arc::new(connectors::MemoryTokenStore::new());
    let keeper = connectors::TokenKeeper::new(tokens, Arc::clone(&http), Arc::new(connectors::SystemClock));
    let hub = connectors::Connectors::from_providers(vec![Arc::new(fixture())], http, keeper);
    assert_eq!(checked(&hub, &[SINK.to_owned()]), Ok(()));
    let refused = checked(&hub, &[SINK.to_owned(), "missing".to_owned()]).expect_err("refused");
    assert!(refused.contains("`missing`") && refused.contains("no connector"), "{refused}");
    let folder = tempfile::tempdir().expect("a folder");
    let store = opened(&folder.path().join("app.db"));
    assert_eq!(start(&store, &Arc::new(hub), &[]), Ok(()));
}
