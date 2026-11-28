use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::binding::Binding;
use super::deliverable::Delivered;
use super::item::{ItemId, ItemRecap, Observed, StepState};
use crate::effect::{EffectOutcome, EffectRequest, IdempotencyKey};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
pub enum WorkflowEvent {
    ItemCreated {
        id: ItemId,
        workflow: String,
        parent: Option<ItemId>,
        input: Value,
    },
    StepStarted {
        step: String,
        branch: Option<String>,
    },
    EffectRequested(EffectRequest),
    EffectResolved {
        key: IdempotencyKey,
        outcome: EffectOutcome,
    },
    EffectProgress {
        key: IdempotencyKey,
        data: Value,
    },
    Signal {
        seq: u64,
        kind: String,
        branch: String,
        step: Option<String>,
        attempt: u32,
        data: Value,
    },
    StepFinished {
        step: String,
        branch: Option<String>,
        state: StepState,
        output: Value,
    },
    StatusSet(String),
    ImpedimentSet {
        on: bool,
        reason: Option<String>,
    },
    HumanInput {
        step: String,
        branch: Option<String>,
        action: String,
        payload: Value,
    },
    ChildStarted {
        child: ItemId,
        workflow: String,
    },
    ChildFinished {
        child: ItemId,
        output: Value,
    },
    TimerSet {
        id: String,
        at: u64,
    },
    TimerFired {
        id: String,
    },
    Rewound {
        to: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        at: Option<u64>,
    },
    Cancelled {
        reason: String,
    },
    Finished {
        output: Value,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        delivered: Option<Delivered>,
    },
    ExternalObserved(Observed),
    Reconciled {
        field: String,
        local: Value,
        remote: Value,
        action: String,
    },
    Named {
        title: String,
    },
    Recapped {
        recap: ItemRecap,
    },
    Linked {
        item: ItemId,
    },
    Unlinked {
        item: ItemId,
    },
    Accepting {
        on: bool,
    },
    Bound {
        binding: Binding,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        keep: bool,
    },
}

const RETIRED: &[&str] = &["heads_up"];

impl WorkflowEvent {
    pub fn decoded(value: Value) -> serde_json::Result<Option<Self>> {
        if value["type"].as_str().is_some_and(|kind| RETIRED.contains(&kind)) {
            return Ok(None);
        }
        serde_json::from_value(value).map(Some)
    }
}
