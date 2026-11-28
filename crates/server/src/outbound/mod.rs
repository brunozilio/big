use std::io::Write;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::{Arc, Mutex, RwLock, Weak};

use domain::redact::{Known, REDACTED, Redactor};
use domain::trace::capture::{self, CONTENT_FIELDS, Keys, Sharing};
use platform::sync::Locked;
use serde_json::{Map, Value};

const REVISION: &str = "policy_revision";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shape {
    Json,
    Text,
}

#[derive(Debug)]
struct Active {
    redactor: Arc<Redactor>,
    revision: String,
}

pub struct Outbound {
    shape: Shape,
    sharing: Sharing,
    keys: Keys,
    active: RwLock<Arc<Active>>,
    sink: Mutex<Box<dyn Write + Send>>,
}

impl std::fmt::Debug for Outbound {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Outbound").field("shape", &self.shape).field("sharing", &self.sharing).finish_non_exhaustive()
    }
}

fn live() -> &'static Mutex<Vec<Weak<Outbound>>> {
    static LIVE: Mutex<Vec<Weak<Outbound>>> = Mutex::new(Vec::new());
    &LIVE
}

pub fn attach(redactor: &Arc<Redactor>) {
    let mut outbounds = live().locked();
    outbounds.retain(|outbound| outbound.strong_count() > 0);
    for outbound in outbounds.iter().filter_map(Weak::upgrade) {
        outbound.redacting(Arc::clone(redactor));
    }
}

impl Outbound {
    pub fn new(shape: Shape, sharing: Sharing, keys: Keys, redactor: Arc<Redactor>, sink: Box<dyn Write + Send>) -> Self {
        let active = Arc::new(Active { revision: capture::revision(keys, sharing, redactor.rules()), redactor });
        Self { shape, sharing, keys, active: RwLock::new(active), sink: Mutex::new(sink) }
    }

    pub fn configured(shape: Shape, sink: Box<dyn Write + Send>) -> Arc<Self> {
        let redactor = connectors::redaction::redactor(std::iter::empty());
        let sharing = Sharing::read(brand::log::SHARING);
        let outbound = Arc::new(Self::new(shape, sharing, Keys::configured(), redactor, sink));
        live().locked().push(Arc::downgrade(&outbound));
        outbound
    }

    pub fn redacting(&self, redactor: Arc<Redactor>) {
        let revision = capture::revision(self.keys, self.sharing, redactor.rules());
        let fresh = Arc::new(Active { redactor, revision });
        match self.active.write() {
            Ok(mut active) => *active = fresh,
            Err(poisoned) => *poisoned.into_inner() = fresh,
        }
    }

    pub fn revision(&self) -> String {
        self.current().revision.clone()
    }

    pub fn line(&self) -> Line<'_> {
        Line { outbound: self, buffer: Vec::new() }
    }

    fn current(&self) -> Arc<Active> {
        match self.active.read() {
            Ok(active) => Arc::clone(&active),
            Err(poisoned) => Arc::clone(&poisoned.into_inner()),
        }
    }

    pub fn cleaned(&self, text: &str) -> Option<String> {
        if self.sharing == Sharing::Disabled {
            return None;
        }
        let active = self.current();
        let cleaned = catch_unwind(AssertUnwindSafe(|| match self.shape {
            Shape::Json => self.json(&active, text),
            Shape::Text => Some(active.redactor.redact(text, &Known::default()).text),
        }));
        let cleaned = cleaned.ok().flatten();
        if cleaned.is_none() {
            capture::drop_one();
        }
        cleaned
    }

    fn json(&self, active: &Active, text: &str) -> Option<String> {
        let Ok(Value::Object(fields)) = serde_json::from_str::<Value>(text.trim_end()) else { return None };
        let shared = |key: &String| self.sharing.carries_content() || !CONTENT_FIELDS.contains(&key.as_str());
        let mut kept: Map<String, Value> = fields
            .into_iter()
            .filter(|(key, _)| shared(key))
            .map(|(key, value)| {
                let value = cleaned_value(&active.redactor, &key, &value);
                (key, value)
            })
            .collect();
        kept.insert(REVISION.to_owned(), Value::String(active.revision.clone()));
        let mut line = serde_json::to_string(&kept).ok()?;
        line.push('\n');
        Some(line)
    }

    fn write(&self, bytes: &[u8]) {
        let text = String::from_utf8_lossy(bytes);
        if let Some(line) = self.cleaned(&text) {
            let mut sink = self.sink.locked();
            let _ = sink.write_all(line.as_bytes()).and_then(|()| sink.flush());
        }
    }
}

fn cleaned_value(redactor: &Redactor, key: &str, value: &Value) -> Value {
    match value {
        Value::String(text) if !text.is_empty() && redactor.secret_key(key) => Value::String(REDACTED.to_owned()),
        other => redactor.redact_value(other, &Known::default()).0,
    }
}

#[derive(Debug)]
pub struct Line<'a> {
    outbound: &'a Outbound,
    buffer: Vec<u8>,
}

impl Write for Line<'_> {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.buffer.extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl Drop for Line<'_> {
    fn drop(&mut self) {
        if !self.buffer.is_empty() {
            self.outbound.write(&std::mem::take(&mut self.buffer));
        }
    }
}

#[cfg(test)]
mod tests;
