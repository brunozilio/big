use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::item::{ItemId, Observed, StepState};
use crate::effect::{EffectOutcome, EffectRequest, IdempotencyKey};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
pub enum WorkflowEvent {
    ItemCreated { id: ItemId, workflow: String, parent: Option<ItemId>, input: Value },
    StepStarted { step: String, branch: Option<String> },
    StepViewSet { step: String, branch: Option<String>, view: Value },
    EffectRequested(EffectRequest),
    EffectResolved { key: IdempotencyKey, outcome: EffectOutcome },
    StepFinished { step: String, branch: Option<String>, state: StepState, output: Value },
    StatusSet(String),
    ImpedimentSet { on: bool, reason: Option<String> },
    HumanInput { step: String, branch: Option<String>, action: String, payload: Value },
    ChildStarted { child: ItemId, workflow: String },
    ChildFinished { child: ItemId, output: Value },
    TimerSet { id: String, at: u64 },
    TimerFired { id: String },
    Rewound { to: String },
    Cancelled { reason: String },
    Finished { output: Value },
    ExternalObserved(Observed),
    Reconciled { field: String, local: Value, remote: Value, action: String },
}
