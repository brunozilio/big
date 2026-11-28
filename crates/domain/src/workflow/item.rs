use std::collections::BTreeMap;
use std::fmt;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use super::deliverable::Delivered;
use crate::effect::{EffectOutcome, EffectRequest, IdempotencyKey};

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ItemId(String);

impl ItemId {
    pub fn new() -> Self {
        Self(Uuid::new_v4().to_string())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<&str> for ItemId {
    fn from(value: &str) -> Self {
        Self(value.to_owned())
    }
}

impl From<String> for ItemId {
    fn from(value: String) -> Self {
        Self(value)
    }
}

impl fmt::Display for ItemId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StepState {
    #[default]
    Pending,
    Running,
    WaitingHuman,
    Passed,
    Failed,
    Skipped,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct StepRun {
    pub id: String,
    pub branch: Option<String>,
    pub state: StepState,
    pub attempt: u32,
    pub view: Value,
    pub output: Value,
}

impl StepRun {
    pub fn is(&self, id: &str, branch: Option<&str>) -> bool {
        self.id == id && self.branch.as_deref() == branch
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ItemRecap {
    pub text: String,
    pub after: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Impediment {
    pub reason: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EffectEntry {
    pub request: EffectRequest,
    pub outcome: Option<EffectOutcome>,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub progress: Value,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ChildLink {
    pub id: ItemId,
    pub workflow: String,
    pub finished: bool,
    pub output: Value,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct HumanAnswer {
    pub step: String,
    pub branch: Option<String>,
    pub action: String,
    pub payload: Value,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Observed {
    pub id: String,
    pub connection: String,
    pub kind: String,
    pub subject: String,
    pub data: Value,
    pub at: i64,
}

impl Observed {
    pub fn slot(&self) -> String {
        format!("{}@{}", self.kind, self.subject)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Ending {
    Finished {
        output: Value,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        delivered: Option<Delivered>,
    },
    Cancelled { reason: String },
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct WorkItem {
    pub id: ItemId,
    pub workflow: String,
    pub input: Value,
    pub parent: Option<ItemId>,
    pub steps: Vec<StepRun>,
    pub status: Option<String>,
    pub impediment: Option<Impediment>,
    pub effects: BTreeMap<String, EffectEntry>,
    pub children: Vec<ChildLink>,
    pub timers: BTreeMap<String, u64>,
    pub inputs: Vec<HumanAnswer>,
    #[serde(default)]
    pub observed: BTreeMap<String, Observed>,
    pub ending: Option<Ending>,
    pub version: u64,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub branch: Option<String>,
    #[serde(default)]
    pub recap: Option<ItemRecap>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub links: Vec<ItemId>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub released: Vec<ItemId>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub accepting: bool,
}

impl WorkItem {
    pub fn step(&self, id: &str, branch: Option<&str>) -> Option<&StepRun> {
        self.steps.iter().find(|step| step.is(id, branch))
    }

    pub fn outcome(&self, key: &IdempotencyKey) -> Option<&EffectOutcome> {
        self.effects.get(&key.to_string()).and_then(|entry| entry.outcome.as_ref())
    }

    pub fn pending_effects(&self) -> impl Iterator<Item = &EffectRequest> {
        self.effects.values().filter(|entry| entry.outcome.is_none()).map(|entry| &entry.request)
    }

    pub fn waiting_human(&self) -> bool {
        self.steps.iter().any(|step| step.state == StepState::WaitingHuman)
    }

    pub const fn is_finished(&self) -> bool {
        matches!(self.ending, Some(Ending::Finished { .. }))
    }

    pub const fn is_cancelled(&self) -> bool {
        matches!(self.ending, Some(Ending::Cancelled { .. }))
    }

    pub fn latest_observed(&self, subject: &str) -> Option<&Observed> {
        self.observed.values().filter(|seen| seen.subject == subject).max_by_key(|seen| seen.at)
    }

    pub const fn is_ended(&self) -> bool {
        self.ending.is_some()
    }
}
