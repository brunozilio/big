mod operation;
mod stage;

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub use operation::OperationTag;
pub use stage::{AuthorizedBy, CreatorType, FailureStage};

pub const SCHEMA_VERSION: u32 = 1;

pub mod code {
    pub const UNATTENDED: &str = "ask.unattended";
    pub const FLOW_UNATTENDED: &str = "flow.unattended";
    pub const GRANTED: &str = "approval.granted";
    pub const REFUSED: &str = "approval.refused";
    pub const ABANDONED: &str = "approval.abandoned";
    pub const STANDING: &str = "standing.allowed";

    pub fn denied(layer: &str) -> String {
        format!("{layer}.denied")
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuditEvent {
    pub schema_version: u32,
    pub at: i64,
    pub action: String,
    pub actor: Option<String>,
    pub subject: Option<String>,
    pub policy: Option<String>,
    pub outcome: Option<String>,
    pub operation: Option<String>,
    pub friendly_name: Option<String>,
    pub policy_version: Option<String>,
    pub decision_code: Option<String>,
    pub layer: Option<String>,
    pub failure_stage: Option<FailureStage>,
    pub approval_wait_ms: Option<u64>,
    pub authorized_by: Option<AuthorizedBy>,
    pub creator_type: Option<CreatorType>,
    pub agent: Option<String>,
    pub detail: Value,
}

impl AuditEvent {
    pub fn new(at: i64, action: impl Into<String>) -> Self {
        Self { schema_version: SCHEMA_VERSION, at, action: action.into(), detail: Value::Null, ..Self::default() }
    }
}

#[cfg(test)]
mod tests;
