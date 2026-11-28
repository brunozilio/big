use std::sync::Arc;
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

use domain::audit::AuditEvent;
use domain::redact::Redactor;
use store::Store;

use super::capture::Captured;
use super::entry::{draft_of, dropped};
use super::queue::Queue;

const TICK: Duration = Duration::from_millis(500);
const BATCH: usize = 256;
const MOST_PENDING: usize = 20_000;
const PRUNE_EVERY: Duration = Duration::from_secs(6 * 3600);

#[derive(Debug)]
pub struct Writer {
    pub store: Arc<Store>,
    pub redactor: Arc<Redactor>,
    pub queue: Arc<Queue>,
    pub retention: Option<Duration>,
    pub sinks: Vec<String>,
}

enum Next {
    Got(Vec<Captured>),
    Idle,
    Closed,
}

fn next(receiver: &Receiver<Captured>) -> Next {
    let first = match receiver.recv_timeout(TICK) {
        Ok(first) => first,
        Err(RecvTimeoutError::Timeout) => return Next::Idle,
        Err(RecvTimeoutError::Disconnected) => return Next::Closed,
    };
    let mut got = vec![first];
    while got.len() < BATCH {
        match receiver.try_recv() {
            Ok(more) => got.push(more),
            Err(_) => break,
        }
    }
    Next::Got(got)
}

fn now() -> i64 {
    jiff::Timestamp::now().as_millisecond()
}

impl Writer {
    pub fn run(self, receiver: &Receiver<Captured>) {
        let mut pending: Vec<AuditEvent> = Vec::new();
        let mut pruned: Option<Instant> = None;
        loop {
            let closed = match next(receiver) {
                Next::Got(got) => {
                    pending.extend(got.into_iter().map(|captured| draft_of(captured, &self.redactor)));
                    false
                }
                Next::Idle => false,
                Next::Closed => true,
            };
            self.write(&mut pending);
            if closed {
                return;
            }
            if pruned.is_none_or(|at| at.elapsed() >= PRUNE_EVERY) {
                self.prune();
                pruned = Some(Instant::now());
            }
        }
    }

    pub(super) fn write(&self, pending: &mut Vec<AuditEvent>) {
        let lost = self.queue.take_dropped();
        if lost > 0 {
            pending.push(dropped(now(), lost));
        }
        if pending.is_empty() {
            return;
        }
        match self.store.append_audit(pending) {
            Ok(_) => pending.clear(),
            Err(error) => {
                tracing::warn!(%error, kept = pending.len(), "the audit trail could not be written yet");
                if pending.len() > MOST_PENDING {
                    let over = pending.len() - MOST_PENDING;
                    pending.drain(..over);
                    self.queue.lost(u64::try_from(over).unwrap_or(u64::MAX));
                }
            }
        }
    }

    fn prune(&self) {
        let Some(older_than) = self.retention else { return };
        let forgotten = if self.sinks.is_empty() {
            self.store.forget_audit(older_than)
        } else {
            self.store.forget_audit_delivered(older_than, &self.sinks)
        };
        match forgotten {
            Ok(0) => {}
            Ok(forgotten) => tracing::info!(forgotten, "audit records past their retention were forgotten"),
            Err(error) => tracing::warn!(%error, "the audit retention could not run"),
        }
    }
}
