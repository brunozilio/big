use std::collections::BTreeMap;
use std::sync::Arc;

use serde_json::Value;
use tracing::field::{Field, Visit};
use tracing::span::{Attributes, Id};
use tracing::{Event, Level, Subscriber};
use tracing_subscriber::Layer;
use tracing_subscriber::layer::Context;
use tracing_subscriber::registry::LookupSpan;

use super::queue::Queue;

pub const TARGET: &str = "audit";
const MESSAGE: &str = "message";

#[derive(Debug, Clone, PartialEq)]
pub struct Captured {
    pub at: i64,
    pub level: Level,
    pub action: String,
    pub fields: BTreeMap<String, Value>,
}

#[derive(Default)]
struct Fields(BTreeMap<String, Value>);

impl Visit for Fields {
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        self.0.insert(field.name().to_owned(), Value::String(format!("{value:?}")));
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        self.0.insert(field.name().to_owned(), Value::String(value.to_owned()));
    }

    fn record_i64(&mut self, field: &Field, value: i64) {
        self.0.insert(field.name().to_owned(), Value::from(value));
    }

    fn record_u64(&mut self, field: &Field, value: u64) {
        self.0.insert(field.name().to_owned(), Value::from(value));
    }

    fn record_f64(&mut self, field: &Field, value: f64) {
        self.0.insert(field.name().to_owned(), Value::from(value));
    }

    fn record_bool(&mut self, field: &Field, value: bool) {
        self.0.insert(field.name().to_owned(), Value::from(value));
    }
}

struct Around(BTreeMap<String, Value>);

#[derive(Debug)]
pub struct Capture {
    queue: Arc<Queue>,
    level: Level,
    skip: Vec<String>,
}

impl Capture {
    pub fn new(queue: Arc<Queue>, level: Level, skip: Vec<String>) -> Self {
        Self { queue, level, skip }
    }

    pub fn filtered<S>(self) -> impl Layer<S> + Send + Sync
    where
        S: Subscriber + for<'a> LookupSpan<'a>,
    {
        let level = self.level;
        let wanted = move |metadata: &tracing::Metadata<'_>| {
            metadata.target() == TARGET && (metadata.is_span() || *metadata.level() <= level)
        };
        self.with_filter(tracing_subscriber::filter::filter_fn(wanted))
    }
}

fn now() -> i64 {
    jiff::Timestamp::now().as_millisecond()
}

impl<S> Layer<S> for Capture
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    fn on_new_span(&self, attrs: &Attributes<'_>, id: &Id, ctx: Context<'_, S>) {
        if attrs.metadata().target() != TARGET {
            return;
        }
        let mut fields = Fields::default();
        attrs.record(&mut fields);
        if let Some(span) = ctx.span(id) {
            span.extensions_mut().insert(Around(fields.0));
        }
    }

    fn on_event(&self, event: &Event<'_>, ctx: Context<'_, S>) {
        let metadata = event.metadata();
        if metadata.target() != TARGET || *metadata.level() > self.level {
            return;
        }
        let mut fields = Fields::default();
        event.record(&mut fields);
        let action = match fields.0.remove(MESSAGE) {
            Some(Value::String(text)) => text,
            _ => metadata.name().to_owned(),
        };
        if self.skip.contains(&action) {
            return;
        }
        if let Some(scope) = ctx.event_scope(event) {
            for span in scope {
                if let Some(around) = span.extensions().get::<Around>() {
                    for (key, value) in &around.0 {
                        fields.0.entry(key.clone()).or_insert_with(|| value.clone());
                    }
                }
            }
        }
        self.queue.offer(Captured { at: now(), level: *metadata.level(), action, fields: fields.0 });
    }
}
