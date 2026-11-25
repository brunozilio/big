use std::collections::BTreeMap;

use serde_json::{Map, Value, json};

use crate::error::ActionError;
use crate::provider::Args;

pub const AWAITING: &str = "awaiting_approval";
pub const EXECUTING: &str = "executing";
pub const DONE: &str = "done";
pub const CANCELLED: &str = "cancelled";
const KEY_BASE: u64 = 4830;
const UPDATABLE: [&str; 7] = ["title", "window", "tasks", "notes", "attachments", "versions", "description"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pace {
    pub approval: i64,
    pub execution: i64,
}

impl Pace {
    pub fn configured() -> Pace {
        let seconds = |text: &str, fallback: i64| text.trim().parse().unwrap_or(fallback);
        Pace {
            approval: seconds(brand::mock_changes::APPROVAL_SECONDS, 30),
            execution: seconds(brand::mock_changes::EXECUTION_SECONDS, 30),
        }
    }

    pub fn status(&self, created: i64, now: i64) -> &'static str {
        let elapsed = now - created;
        if elapsed < self.approval {
            AWAITING
        } else if elapsed < self.approval + self.execution {
            EXECUTING
        } else {
            DONE
        }
    }

    fn turns(&self, created: i64) -> [(i64, &'static str); 2] {
        [(created + self.approval, EXECUTING), (created + self.approval + self.execution, DONE)]
    }
}

#[derive(Debug, Clone, PartialEq)]
struct Change {
    created: i64,
    seq: u64,
    revision: u64,
    cancelled: bool,
    data: Map<String, Value>,
}

#[derive(Debug, Default)]
pub struct Ledger {
    seq: u64,
    keys: BTreeMap<String, String>,
    changes: BTreeMap<String, Change>,
}

fn parse_ref(reference: &str) -> Option<(i64, u64)> {
    let rest = reference.strip_prefix("mock-")?;
    let (created, seq) = rest.split_once('-')?;
    Some((created.parse().ok()?, seq.parse().ok()?))
}

fn unknown(reference: &str) -> ActionError {
    ActionError::permanent(format!("the change {reference} does not exist"))
}

impl Ledger {
    fn entry(&mut self, reference: &str) -> Result<&mut Change, ActionError> {
        if !self.changes.contains_key(reference) {
            let (created, seq) = parse_ref(reference).ok_or_else(|| unknown(reference))?;
            let restored = Change { created, seq, revision: 0, cancelled: false, data: Map::new() };
            self.changes.insert(reference.to_owned(), restored);
        }
        self.changes.get_mut(reference).ok_or_else(|| unknown(reference))
    }

    fn shown(reference: &str, change: &Change, pace: Pace, now: i64) -> Value {
        let status = if change.cancelled { CANCELLED } else { pace.status(change.created, now) };
        let mut out = json!({
            "ref": reference,
            "key": format!("GMUD-{}", KEY_BASE + change.seq),
            "status": status,
            "revision": change.revision,
        });
        for name in ["title", "environment", "window"] {
            if let Some(value) = change.data.get(name) {
                out[name] = value.clone();
            }
        }
        out
    }

    pub fn create(&mut self, args: Args<'_>, key: &str, pace: Pace, now: i64) -> Result<Value, ActionError> {
        for name in ["title", "environment", "window"] {
            args.text(name)?;
        }
        if let Some(reference) = self.keys.get(key).filter(|_| !key.is_empty()).cloned() {
            let change = self.entry(&reference)?.clone();
            let mut out = Ledger::shown(&reference, &change, pace, now);
            out["created"] = json!(false);
            return Ok(out);
        }
        self.seq += 1;
        let reference = format!("mock-{now}-{}", self.seq);
        let data = args.0.as_object().cloned().unwrap_or_default();
        let change = Change { created: now, seq: self.seq, revision: 0, cancelled: false, data };
        let mut out = Ledger::shown(&reference, &change, pace, now);
        out["created"] = json!(true);
        self.changes.insert(reference.clone(), change);
        if !key.is_empty() {
            self.keys.insert(key.to_owned(), reference);
        }
        Ok(out)
    }

    pub fn update(&mut self, args: Args<'_>, pace: Pace, now: i64) -> Result<Value, ActionError> {
        let reference = args.text("change")?;
        let change = self.entry(reference)?;
        if change.cancelled {
            return Err(ActionError::permanent(format!("the change {reference} was cancelled")));
        }
        for name in UPDATABLE {
            if let Some(value) = args.0.get(name).filter(|value| !value.is_null()) {
                change.data.insert(name.to_owned(), value.clone());
            }
        }
        change.revision += 1;
        let change = change.clone();
        Ok(Ledger::shown(reference, &change, pace, now))
    }

    pub fn get(&mut self, args: Args<'_>, pace: Pace, now: i64) -> Result<Value, ActionError> {
        let reference = args.text("change")?;
        let change = self.entry(reference)?.clone();
        Ok(Ledger::shown(reference, &change, pace, now))
    }

    pub fn cancel(&mut self, args: Args<'_>, pace: Pace, now: i64) -> Result<Value, ActionError> {
        let reference = args.text("change")?;
        let change = self.entry(reference)?;
        change.cancelled = true;
        let change = change.clone();
        Ok(Ledger::shown(reference, &change, pace, now))
    }

    pub fn moved(&self, pace: Pace, since: i64, now: i64) -> Vec<(String, &'static str, i64)> {
        let mut found = Vec::new();
        for (reference, change) in self.changes.iter().filter(|(_, change)| !change.cancelled) {
            for (at, status) in pace.turns(change.created) {
                if at > since && at <= now {
                    found.push((reference.clone(), status, at));
                }
            }
        }
        found.sort_by_key(|(_, _, at)| *at);
        found
    }
}
