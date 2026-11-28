use std::sync::Arc;
use std::time::Duration;

use connectors::ActionError;
use serde_json::Value;
use store::Store;

use super::backoff::Pace;
use super::event::event_of;
use super::sink::Sink;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    Delivered { through: u64, acked: usize, rejected: usize },
    Idle,
    Failed { wait: Duration },
}

#[derive(Debug)]
pub struct Outbox {
    store: Arc<Store>,
    sink: Arc<dyn Sink>,
    pace: Pace,
    failures: u32,
}

impl Outbox {
    pub fn new(store: Arc<Store>, sink: Arc<dyn Sink>, pace: Pace) -> Outbox {
        Outbox { store, sink, pace, failures: 0 }
    }

    pub async fn step(&mut self) -> Step {
        let name = self.sink.name().to_owned();
        let (delivery, pending) = match self.store.audit_outbox(&name, self.pace.offer) {
            Ok(found) => found,
            Err(error) => return self.failed(&error.to_string(), None),
        };
        let (Some(first), Some(last)) = (pending.first(), pending.last()) else { return Step::Idle };
        let events: Result<Vec<Value>, ActionError> = pending.iter().map(event_of).collect();
        let events = match events {
            Ok(events) => events,
            Err(error) => return self.failed(&error.message, None),
        };
        let key = format!("audit:{name}:{}-{}", first.seq, last.seq);
        let ack = match self.sink.deliver(&events, &key).await {
            Ok(ack) if ack.taken() > 0 => ack,
            Ok(_) => return self.failed("the sink took no event", None),
            Err(error) => return self.failed(&error.message, self.pace.rate_limited(&error.class)),
        };
        let Some(taken) = pending.get(ack.taken() - 1) else {
            return self.failed("the sink took more than offered", None);
        };
        let through = taken.seq;
        let dropped = u64::try_from(ack.rejected).unwrap_or(u64::MAX);
        match self.store.audit_delivered(&name, delivery.delivered, through, dropped) {
            Ok(true) => {
                self.failures = 0;
                if ack.rejected > 0 {
                    tracing::warn!(sink = name, rejected = ack.rejected, through, "the sink rejected audit events");
                }
                Step::Delivered { through, acked: ack.ack, rejected: ack.rejected }
            }
            Ok(false) => Step::Idle,
            Err(error) => self.failed(&error.to_string(), None),
        }
    }

    fn failed(&mut self, why: &str, at_least: Option<Duration>) -> Step {
        self.failures = self.failures.saturating_add(1);
        let name = self.sink.name();
        if let Err(error) = self.store.audit_undelivered(name, why) {
            tracing::warn!(sink = name, %error, "the audit delivery failure could not be recorded");
        }
        let wait = self.pace.backoff(self.failures).max(at_least.unwrap_or_default());
        tracing::warn!(
            sink = name,
            why,
            attempts = self.failures,
            wait_ms = wait.as_millis(),
            "audit events wait for the sink"
        );
        Step::Failed { wait }
    }

    pub async fn run(mut self) {
        loop {
            match self.step().await {
                Step::Delivered { .. } => tokio::task::yield_now().await,
                Step::Idle => tokio::time::sleep(self.pace.idle).await,
                Step::Failed { wait } => tokio::time::sleep(wait).await,
            }
        }
    }
}
