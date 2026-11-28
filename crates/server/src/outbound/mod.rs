mod ordered;
mod redacts;

use std::cell::Cell;
use std::io::Write;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::{Arc, Mutex, RwLock, Weak};

use domain::redact::Redactor;
use domain::trace::capture::{self, CONTENT_FIELDS, Keys, Sharing};
use platform::sync::Locked;
use serde_json::Value;

use self::ordered::Ordered;
pub use self::redacts::Redacts;

const REVISION: &str = "policy_revision";

thread_local! {
    static CLEANING: Cell<bool> = const { Cell::new(false) };
}

struct Cleaning;

impl Cleaning {
    fn enter() -> Option<Cleaning> {
        let busy = CLEANING.with(|cleaning| cleaning.replace(true));
        (!busy).then_some(Cleaning)
    }
}

impl Drop for Cleaning {
    fn drop(&mut self) {
        CLEANING.with(|cleaning| cleaning.set(false));
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shape {
    Json,
    Text,
}

struct Active {
    redactor: Arc<dyn Redacts>,
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
        outbound.redacting(Arc::clone(redactor) as Arc<dyn Redacts>);
    }
}

impl Outbound {
    pub fn new(shape: Shape, sharing: Sharing, keys: Keys, redactor: Arc<dyn Redacts>, sink: Box<dyn Write + Send>) -> Self {
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

    pub fn redacting(&self, redactor: Arc<dyn Redacts>) {
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
        let Some(_cleaning) = Cleaning::enter() else {
            capture::drop_one();
            return None;
        };
        let active = self.current();
        let cleaned = catch_unwind(AssertUnwindSafe(|| match self.shape {
            Shape::Json => self.json(&active, text),
            Shape::Text => Some(active.redactor.text(text)),
        }));
        let cleaned = cleaned.ok().flatten();
        if cleaned.is_none() {
            capture::drop_one();
        }
        cleaned
    }

    fn json(&self, active: &Active, text: &str) -> Option<String> {
        let Ok(Ordered(fields)) = serde_json::from_str::<Ordered>(text.trim_end()) else { return None };
        let shared = |key: &String| self.sharing.carries_content() || !CONTENT_FIELDS.contains(&key.as_str());
        let mut kept: Vec<(String, Value)> = fields
            .into_iter()
            .filter(|(key, _)| shared(key) && key != REVISION)
            .map(|(key, value)| {
                let value = active.redactor.value(&key, &value);
                (key, value)
            })
            .collect();
        kept.push((REVISION.to_owned(), Value::String(active.revision.clone())));
        let mut line = serde_json::to_string(&Ordered(kept)).ok()?;
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
