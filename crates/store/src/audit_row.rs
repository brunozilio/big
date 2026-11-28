use database::{Param, Row, Value};
use domain::audit::{AuditEvent, CreatorType, FailureStage, SCHEMA_VERSION};

use crate::{Result, unsigned};

pub(crate) const COLUMNS: &str = "seq, at, actor, subject, action, policy, outcome, detail, prev, hash, \
     schema_version, operation, friendly_name, policy_version, decision_code, layer, failure_stage, \
     approval_wait_ms, authorized_by, creator_type, agent";

#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct Stored {
    pub seq: u64,
    pub at: i64,
    pub actor: Option<String>,
    pub subject: Option<String>,
    pub action: String,
    pub policy: Option<String>,
    pub outcome: Option<String>,
    pub detail: String,
    pub prev: String,
    pub hash: String,
    pub schema_version: u32,
    pub operation: Option<String>,
    pub friendly_name: Option<String>,
    pub policy_version: Option<String>,
    pub decision_code: Option<String>,
    pub layer: Option<String>,
    pub failure_stage: Option<String>,
    pub approval_wait_ms: Option<u64>,
    pub authorized_by: Option<String>,
    pub creator_type: Option<String>,
    pub agent: Option<String>,
}

impl Stored {
    pub fn of_event(seq: u64, event: &AuditEvent) -> Self {
        Self {
            seq,
            at: event.at,
            actor: event.actor.clone(),
            subject: event.subject.clone(),
            action: event.action.clone(),
            policy: event.policy.clone(),
            outcome: event.outcome.clone(),
            detail: event.detail.to_string(),
            prev: String::new(),
            hash: String::new(),
            schema_version: SCHEMA_VERSION,
            operation: event.operation.clone(),
            friendly_name: event.friendly_name.clone(),
            policy_version: event.policy_version.clone(),
            decision_code: event.decision_code.clone(),
            layer: event.layer.clone(),
            failure_stage: event.failure_stage.map(|stage| stage.word().to_owned()),
            approval_wait_ms: event.approval_wait_ms,
            authorized_by: event.authorized_by.as_ref().map(ToString::to_string),
            creator_type: event.creator_type.map(|kind| kind.word().to_owned()),
            agent: event.agent.clone(),
        }
    }

    pub fn read(row: &Row) -> Result<Self> {
        let wait: Option<i64> = row.get(17)?;
        Ok(Self {
            seq: unsigned(row.get(0)?)?,
            at: row.get(1)?,
            actor: row.get(2)?,
            subject: row.get(3)?,
            action: row.get(4)?,
            policy: row.get(5)?,
            outcome: row.get(6)?,
            detail: row.get(7)?,
            prev: row.get(8)?,
            hash: row.get(9)?,
            schema_version: row.get(10)?,
            operation: row.get(11)?,
            friendly_name: row.get(12)?,
            policy_version: row.get(13)?,
            decision_code: row.get(14)?,
            layer: row.get(15)?,
            failure_stage: row.get(16)?,
            approval_wait_ms: wait.map(unsigned).transpose()?,
            authorized_by: row.get(18)?,
            creator_type: row.get(19)?,
            agent: row.get(20)?,
        })
    }

    pub fn event(&self) -> Result<AuditEvent> {
        Ok(AuditEvent {
            schema_version: self.schema_version,
            at: self.at,
            action: self.action.clone(),
            actor: self.actor.clone(),
            subject: self.subject.clone(),
            policy: self.policy.clone(),
            outcome: self.outcome.clone(),
            operation: self.operation.clone(),
            friendly_name: self.friendly_name.clone(),
            policy_version: self.policy_version.clone(),
            decision_code: self.decision_code.clone(),
            layer: self.layer.clone(),
            failure_stage: self.failure_stage.as_deref().and_then(FailureStage::named),
            approval_wait_ms: self.approval_wait_ms,
            authorized_by: self.authorized_by.as_deref().and_then(|text| text.parse().ok()),
            creator_type: self.creator_type.as_deref().and_then(CreatorType::named),
            agent: self.agent.clone(),
            detail: serde_json::from_str(&self.detail)?,
        })
    }

    pub fn values(&self) -> Vec<Value> {
        vec![
            self.seq.value(),
            self.at.value(),
            self.actor.value(),
            self.subject.value(),
            self.action.value(),
            self.policy.value(),
            self.outcome.value(),
            self.detail.value(),
            self.prev.value(),
            self.hash.value(),
            self.schema_version.value(),
            self.operation.value(),
            self.friendly_name.value(),
            self.policy_version.value(),
            self.decision_code.value(),
            self.layer.value(),
            self.failure_stage.value(),
            self.approval_wait_ms.value(),
            self.authorized_by.value(),
            self.creator_type.value(),
            self.agent.value(),
        ]
    }
}
