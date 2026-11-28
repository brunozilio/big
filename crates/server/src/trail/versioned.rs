use std::collections::BTreeMap;

use domain::audit::{AuditEvent, CreatorType, FailureStage};
use domain::redact::Redactor;
use serde_json::Value;

use super::entry::{taken, word};

const OPERATION: &[&str] = &["operation.id"];
const OPERATION_VERSION: &[&str] = &["operation.version"];
const FRIENDLY_NAME: &[&str] = &["operation.name", "friendly_name"];
const POLICY_VERSION: &[&str] = &["policy.version"];
const DECISION_CODE: &[&str] = &["decision.code"];
const FAILURE_STAGE: &[&str] = &["failure.stage"];
const ERROR_KIND: &str = "error.kind";
const APPROVAL_WAIT: &[&str] = &["approval.wait_ms"];
const AUTHORIZED_BY: &[&str] = &["authorized_by"];
const CREATOR: &[&str] = &["creator.type"];
const AGENT_VERSION: &[&str] = &["agent.version"];
const SURFACE: &str = "surface";
const DENIED: &str = "denied";

pub fn filled(event: &mut AuditEvent, fields: &mut BTreeMap<String, Value>, redactor: &Redactor) {
    let operation = taken(fields, OPERATION, redactor);
    let version = taken(fields, OPERATION_VERSION, redactor);
    event.operation = operation.map(|id| version.map_or_else(|| id.clone(), |version| format!("{id}@{version}")));
    event.friendly_name = taken(fields, FRIENDLY_NAME, redactor);
    event.policy_version = taken(fields, POLICY_VERSION, redactor);
    event.decision_code = taken(fields, DECISION_CODE, redactor);
    event.approval_wait_ms = APPROVAL_WAIT.iter().find_map(|key| fields.remove(*key)).and_then(|wait| wait.as_u64());
    event.authorized_by = taken(fields, AUTHORIZED_BY, redactor).and_then(|text| text.parse().ok());
    let agent_version = taken(fields, AGENT_VERSION, redactor);
    event.agent = event.actor.clone().map(|agent| match agent_version {
        Some(version) => format!("{agent}@{version}"),
        None => agent,
    });
    event.failure_stage = stage_of(event, fields, redactor);
    event.creator_type = creator_of(event, fields, redactor);
}

fn stage_of(event: &AuditEvent, fields: &mut BTreeMap<String, Value>, redactor: &Redactor) -> Option<FailureStage> {
    if let Some(stage) = taken(fields, FAILURE_STAGE, redactor).as_deref().and_then(FailureStage::named) {
        return Some(stage);
    }
    if event.outcome.as_deref() == Some(DENIED) {
        return Some(FailureStage::Authorization);
    }
    fields.get(ERROR_KIND).and_then(word).as_deref().and_then(FailureStage::of_kind)
}

fn creator_of(event: &AuditEvent, fields: &mut BTreeMap<String, Value>, redactor: &Redactor) -> Option<CreatorType> {
    if let Some(kind) = taken(fields, CREATOR, redactor).as_deref().and_then(CreatorType::named) {
        return Some(kind);
    }
    let surface = fields.get(SURFACE).and_then(word);
    Some(match surface.as_deref() {
        Some("workflow" | "routine") => CreatorType::Automation,
        Some("chat" | "mcp") => CreatorType::Agent,
        _ if event.actor.is_some() => CreatorType::Agent,
        _ if event.subject.is_some() => CreatorType::Person,
        _ => CreatorType::Automation,
    })
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use tracing::Level;

    use super::*;
    use crate::trail::capture::Captured;
    use crate::trail::entry::draft_of;

    fn drafted(fields: Value) -> AuditEvent {
        let fields = fields.as_object().cloned().unwrap_or_default().into_iter().collect();
        draft_of(Captured { at: 9, level: Level::WARN, action: "policy.blocked".into(), fields }, &Redactor::default())
    }

    #[test]
    fn a_blocked_call_carries_its_code_layer_and_the_authorization_stage() {
        let event = drafted(json!({
            "surface": "workflow", "user": "u-ana", "layer": "floor", "outcome": "denied",
            "decision.code": "floor.denied", "policy.version": "abc", "operation.id": "orders.approve",
            "operation.version": "3", "operation.name": "Approve order"
        }));
        assert_eq!(event.schema_version, domain::audit::SCHEMA_VERSION);
        assert_eq!(event.decision_code.as_deref(), Some("floor.denied"));
        assert_eq!(event.layer.as_deref(), Some("floor"));
        assert_eq!(event.policy.as_deref(), Some("floor"));
        assert_eq!(event.failure_stage, Some(FailureStage::Authorization));
        assert_eq!(event.operation.as_deref(), Some("orders.approve@3"));
        assert_eq!(event.friendly_name.as_deref(), Some("Approve order"));
        assert_eq!(event.policy_version.as_deref(), Some("abc"));
        assert_eq!(event.creator_type, Some(CreatorType::Automation));
        assert_eq!(event.detail, json!({ "surface": "workflow", "level": "warn" }));
    }

    #[test]
    fn an_approved_call_says_which_approval_and_how_long_it_waited() {
        let event =
            drafted(json!({ "authorized_by": "approval:policy-1", "approval.wait_ms": 820, "outcome": "success" }));
        assert_eq!(event.authorized_by, Some(domain::audit::AuthorizedBy::Approval("policy-1".into())));
        assert_eq!(event.approval_wait_ms, Some(820));
        assert_eq!(event.failure_stage, None);
    }

    #[test]
    fn the_agent_carries_its_version_and_a_failure_kind_names_the_stage() {
        let event = drafted(json!({ "agent": "dev", "agent.version": "9f3a", "error.kind": "rate_limited" }));
        assert_eq!(event.agent.as_deref(), Some("dev@9f3a"));
        assert_eq!(event.actor.as_deref(), Some("dev"));
        assert_eq!(event.creator_type, Some(CreatorType::Agent));
        assert_eq!(event.failure_stage, Some(FailureStage::Throttling));
        let person = drafted(json!({ "user": "u-bia", "failure.stage": "authentication" }));
        assert_eq!(person.creator_type, Some(CreatorType::Person));
        assert_eq!(person.failure_stage, Some(FailureStage::Authentication));
    }
}
