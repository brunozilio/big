use std::collections::{BTreeSet, HashMap};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use connectors::contracts::{CODE, PIPELINES, TRACKER};
use connectors::{Connectors, ExternalEvent};
use domain::connection::{Capability, ConnectionId};
use domain::effect::{ErrorClass, next_retry};
use domain::workflow::{Observed, WorkflowEvent};
use serde_json::{Value, json};
use store::Store;
use tokio::sync::Notify;
use tokio::time::Instant;

use crate::book::{Book, Entry};
use crate::config::SyncConfig;
use crate::desired::Desired;
use crate::error::Error;
use crate::health::Health;
use crate::sink::EventSink;
use crate::view::ItemView;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PollReport {
    pub fetched: usize,
    pub duplicates: usize,
    pub delivered: usize,
    pub triggered: usize,
    pub observed: usize,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ReconcileReport {
    pub items: usize,
    pub corrections: usize,
    pub errors: Vec<String>,
}

#[derive(Debug, Default)]
pub struct TickReport {
    pub polled: Vec<(ConnectionId, Result<Option<PollReport>, String>)>,
    pub reconciled: Option<ReconcileReport>,
}

#[derive(Debug, Default)]
struct Schedule {
    due: HashMap<ConnectionId, Instant>,
    reconcile_at: Option<Instant>,
}

#[derive(Debug)]
pub struct Synchronizer {
    pub(crate) store: Arc<Store>,
    pub(crate) connectors: Arc<Connectors>,
    pub(crate) sink: Arc<dyn EventSink>,
    view: Arc<dyn ItemView>,
    pub(crate) health: Arc<Health>,
    config: SyncConfig,
    schedule: Mutex<Schedule>,
    wake: Notify,
    stopped: AtomicBool,
}

pub(crate) fn observed(event: &ExternalEvent) -> Observed {
    Observed {
        id: event.id.clone(),
        connection: event.connection.to_string(),
        kind: event.kind.clone(),
        subject: event.subject.clone(),
        data: event.data.clone(),
        at: event.at.as_millisecond(),
    }
}

pub(crate) fn reconciled(field: &str, local: Value, remote: Value, action: &str) -> WorkflowEvent {
    WorkflowEvent::Reconciled { field: field.to_owned(), local, remote, action: action.to_owned() }
}

pub(crate) fn linked(desired: &Desired, book: &Book) -> BTreeSet<ConnectionId> {
    let issue = desired.issue.iter().filter_map(|issue| book.resolve(issue.connection.as_deref(), TRACKER));
    let pulls = desired.pulls.iter().filter_map(|pull| book.resolve(pull.connection.as_deref(), CODE));
    let runs = desired.runs.iter().filter_map(|run| book.resolve(run.connection.as_deref(), PIPELINES));
    issue.chain(pulls).chain(runs).map(|entry| entry.connection.id.clone()).collect()
}

impl Synchronizer {
    pub fn new(
        store: Arc<Store>,
        connectors: Arc<Connectors>,
        sink: Arc<dyn EventSink>,
        view: Arc<dyn ItemView>,
        config: SyncConfig,
    ) -> Synchronizer {
        Synchronizer {
            store,
            connectors,
            sink,
            view,
            health: Arc::new(Health::new()),
            config,
            schedule: Mutex::default(),
            wake: Notify::new(),
            stopped: AtomicBool::new(false),
        }
    }

    pub fn health(&self) -> &Arc<Health> {
        &self.health
    }

    pub fn connectors(&self) -> &Arc<Connectors> {
        &self.connectors
    }

    pub fn config(&self) -> &SyncConfig {
        &self.config
    }

    fn schedule(&self) -> std::sync::MutexGuard<'_, Schedule> {
        self.schedule.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub fn hurry(&self) {
        let mut schedule = self.schedule();
        schedule.due.clear();
        drop(schedule);
        self.wake.notify_one();
    }

    pub fn reconcile_soon(&self) {
        self.schedule().reconcile_at = None;
        self.wake.notify_one();
    }

    pub fn stop(&self) {
        self.stopped.store(true, Ordering::SeqCst);
        self.wake.notify_one();
    }

    pub(crate) async fn active(&self) -> Result<Vec<Desired>, Error> {
        self.view.active().await
    }

    pub fn due(&self, connection: &ConnectionId) -> Option<Instant> {
        self.schedule().due.get(connection).copied()
    }

    pub async fn run(&self) {
        while !self.stopped.load(Ordering::SeqCst) {
            let wake = match self.tick().await {
                Ok((_, next)) => next,
                Err(_) => Instant::now() + self.config.retry.initial,
            };
            let _ = tokio::time::timeout_at(wake, self.wake.notified()).await;
        }
    }

    pub async fn tick(&self) -> Result<(TickReport, Instant), Error> {
        let book = Book::load(&self.store)?;
        let desired = self.view.active().await?;
        let mut report = TickReport::default();
        for entry in &book.entries {
            let id = &entry.connection.id;
            if self.due(id).is_some_and(|due| due > Instant::now()) {
                continue;
            }
            let (outcome, delay) = self.poll_entry(entry, &book, &desired).await;
            self.schedule().due.insert(id.clone(), Instant::now() + delay);
            report.polled.push((id.clone(), outcome));
        }
        if self.schedule().reconcile_at.is_none_or(|at| at <= Instant::now()) {
            let desired = self.view.active().await?;
            report.reconciled = Some(self.reconcile_with(&book, &desired).await);
            self.schedule().reconcile_at = Some(Instant::now() + self.config.reconcile_every);
        }
        Ok((report, self.next_wake(&book)))
    }

    fn next_wake(&self, book: &Book) -> Instant {
        let now = Instant::now();
        let schedule = self.schedule();
        let polls = book.entries.iter().map(|entry| schedule.due.get(&entry.connection.id).copied().unwrap_or(now));
        polls.chain(schedule.reconcile_at).min().unwrap_or(now + self.config.slow_poll)
    }

    pub(crate) fn interval(&self, entry: &Entry, desired: &[Desired]) -> Duration {
        if desired.iter().any(|item| item.waits_on(&entry.connection)) {
            self.config.fast_poll
        } else {
            self.config.slow_poll
        }
    }

    async fn poll_entry(
        &self,
        entry: &Entry,
        book: &Book,
        desired: &[Desired],
    ) -> (Result<Option<PollReport>, String>, Duration) {
        let id = &entry.connection.id;
        if let Some(wait) = self.connectors.limiter().waiting(id) {
            return (Ok(None), wait);
        }
        match self.poll_connection(entry, desired).await {
            Ok(report) => {
                if report.is_some() {
                    self.health.healthy(id);
                }
                (Ok(report), self.interval(entry, desired))
            }
            Err(error) => {
                let delay = self.after_failure(entry, book, desired, &error).await;
                (Err(error.to_string()), delay)
            }
        }
    }

    async fn after_failure(&self, entry: &Entry, book: &Book, desired: &[Desired], error: &Error) -> Duration {
        let id = &entry.connection.id;
        let class = error.class();
        if class == ErrorClass::Auth {
            let _ = self.revoke(id, &error.to_string(), book, desired).await;
            return self.config.slow_poll;
        }
        let attempt = self.health.failing(id, &error.to_string());
        let delay = next_retry(&self.config.retry, attempt, &class).unwrap_or(self.config.slow_poll);
        let held = self.connectors.limiter().waiting(id).unwrap_or_default();
        delay.max(held).max(self.config.retry.initial.min(self.config.fast_poll))
    }

    pub(crate) async fn revoke(
        &self,
        id: &ConnectionId,
        reason: &str,
        book: &Book,
        desired: &[Desired],
    ) -> Result<(), Error> {
        if !self.health.revoked(id, reason) {
            return Ok(());
        }
        for item in desired.iter().filter(|item| linked(item, book).contains(id)) {
            let event = reconciled("connection", json!(id), json!("revoked"), "reconnect");
            self.sink.record(&item.item, &[event]).await?;
        }
        Ok(())
    }

    pub(crate) fn synthetic(
        entry: &Entry,
        capability: &str,
        kind: &str,
        subject: &str,
        data: Value,
        id: String,
    ) -> ExternalEvent {
        ExternalEvent {
            id,
            connection: entry.connection.id.clone(),
            capability: Capability::from(capability),
            kind: kind.to_owned(),
            subject: subject.to_owned(),
            data,
            at: jiff::Timestamp::now(),
        }
    }
}
