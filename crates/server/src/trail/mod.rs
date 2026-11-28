mod capture;
mod entry;
mod queue;
mod versioned;
mod writer;

use std::sync::{Arc, OnceLock};
use std::time::Duration;

use domain::redact::Redactor;
use store::Store;
use tracing::{Level, Subscriber};
use tracing_subscriber::Layer;
use tracing_subscriber::registry::LookupSpan;

pub use capture::{Capture, Captured, TARGET};
pub use queue::Queue;
pub use writer::Writer;

const DAY: u64 = 24 * 3600;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settings {
    pub level: Level,
    pub skip: Vec<String>,
    pub capacity: usize,
    pub retention: Option<Duration>,
}

impl Settings {
    pub fn configured() -> Self {
        Self::read(brand::audit::LEVEL, brand::audit::SKIP, brand::audit::QUEUE, brand::audit::RETENTION_DAYS)
    }

    pub fn read(level: &str, skip: &str, capacity: &str, retention_days: &str) -> Self {
        let level = level.trim().parse().unwrap_or(Level::INFO);
        let skip = skip.split([' ', ',']).filter(|word| !word.is_empty()).map(str::to_owned).collect();
        let capacity = capacity.trim().parse().unwrap_or(10_000);
        let days: Option<u64> = retention_days.trim().parse().ok().filter(|days| *days > 0);
        Self { level, skip, capacity, retention: days.map(|days| Duration::from_secs(days * DAY)) }
    }
}

fn settings() -> &'static Settings {
    static SETTINGS: OnceLock<Settings> = OnceLock::new();
    SETTINGS.get_or_init(Settings::configured)
}

fn queue() -> &'static Arc<Queue> {
    static QUEUE: OnceLock<Arc<Queue>> = OnceLock::new();
    QUEUE.get_or_init(|| Arc::new(Queue::new(settings().capacity)))
}

pub fn layer<S>() -> impl Layer<S> + Send + Sync
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    let settings = settings();
    Capture::new(Arc::clone(queue()), settings.level, settings.skip.clone()).filtered()
}

pub fn attach(store: Arc<Store>, redactor: Arc<Redactor>) {
    let queue = Arc::clone(queue());
    let Some(receiver) = queue.take_receiver() else { return };
    let writer = Writer { store, redactor, queue, retention: settings().retention };
    let started = std::thread::Builder::new().name("audit-trail".to_owned()).spawn(move || writer.run(&receiver));
    if let Err(error) = started {
        tracing::warn!(%error, "the audit trail writer did not start");
    }
}

#[cfg(test)]
mod tests;
