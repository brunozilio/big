use serde_json::json;

use super::*;
use crate::factory::ENVIRONMENT;

#[test]
fn the_factory_failure_kinds_map_to_a_stage_or_to_none() {
    let mapped: Vec<(&str, Option<FailureStage>)> =
        ENVIRONMENT.iter().map(|kind| (*kind, FailureStage::of_kind(kind))).collect();
    assert_eq!(
        mapped,
        [
            ("timeout", Some(FailureStage::Timeout)),
            ("transient", Some(FailureStage::Dependency)),
            ("rate_limited", Some(FailureStage::Throttling)),
            ("auth", Some(FailureStage::Authentication)),
            ("conflict", Some(FailureStage::Validation)),
            ("permanent", None),
            ("cancelled", None),
        ]
    );
}

#[test]
fn every_stage_reads_back_from_its_word() {
    for stage in FailureStage::ALL {
        assert_eq!(FailureStage::named(stage.word()), Some(stage));
        assert_eq!(serde_json::to_value(stage).ok(), Some(json!(stage.word())));
    }
    assert_eq!(FailureStage::named("nope"), None);
}

#[test]
fn authorized_by_is_standing_or_an_approval_id() {
    let approved = AuthorizedBy::Approval("policy-7".into());
    assert_eq!(approved.to_string(), "approval:policy-7");
    assert_eq!("approval:policy-7".parse::<AuthorizedBy>(), Ok(approved));
    assert_eq!("standing".parse::<AuthorizedBy>(), Ok(AuthorizedBy::Standing));
    assert!("approval:".parse::<AuthorizedBy>().is_err());
    assert!("someone".parse::<AuthorizedBy>().is_err());
}

#[test]
fn the_event_serializes_with_its_schema_version_in_camel_case() {
    let mut event = AuditEvent::new(5, "operation.call");
    event.authorized_by = Some(AuthorizedBy::Approval("a1".into()));
    event.approval_wait_ms = Some(1200);
    event.creator_type = Some(CreatorType::Automation);
    let shown = serde_json::to_value(&event).unwrap_or_default();
    assert_eq!(shown["schemaVersion"], SCHEMA_VERSION);
    assert_eq!(shown["authorizedBy"], "approval:a1");
    assert_eq!(shown["approvalWaitMs"], 1200);
    assert_eq!(shown["creatorType"], "automation");
    let back: AuditEvent = serde_json::from_value(shown).unwrap_or_default();
    assert_eq!(back, event);
}

#[test]
fn an_operation_tag_reads_a_number_or_text_version() {
    let tag = OperationTag::read(&json!({ "id": "orders.approve", "version": 3, "friendlyName": "Approve order" }));
    let tag = tag.unwrap_or_else(|| OperationTag::plain("missing"));
    assert_eq!(tag.label(), "orders.approve@3");
    assert_eq!(tag.friendly_name.as_deref(), Some("Approve order"));
    let texted = OperationTag::read(&json!({ "id": "a", "version": "2.1" })).map(|tag| tag.label());
    assert_eq!(texted.as_deref(), Some("a@2.1"));
    assert_eq!(OperationTag::plain("b").label(), "b");
    assert_eq!(OperationTag::read(&json!({ "id": " " })), None);
    assert_eq!(OperationTag::read(&json!("x")), None);
}
