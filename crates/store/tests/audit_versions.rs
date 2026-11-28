#![allow(clippy::expect_used, reason = "a failed setup step stops the test")]

#[macro_use]
mod common;

use std::fmt::Write as _;

use common::{Fresh, Place};
use database::args;
use domain::audit::{AuditEvent, AuthorizedBy, CreatorType, FailureStage, SCHEMA_VERSION};
use serde_json::json;
use sha2::{Digest, Sha256};
use store::{AuditFilter, AuditPage, GENESIS};

fn legacy_hash(prev: &str, seq: u64, at: i64, action: &str, outcome: Option<&str>, detail: &str) -> String {
    let fields = json!([prev, seq, at, "dev", "ana", action, "floor", outcome, detail]);
    Sha256::digest(fields.to_string().as_bytes()).iter().fold(String::new(), |mut out, byte| {
        let _ = write!(out, "{byte:02x}");
        out
    })
}

fn legacy(place: &Place) -> String {
    let mut prev = GENESIS.to_owned();
    for (seq, action, outcome) in [(1_u64, "tool.call", Some("success")), (2, "policy.blocked", Some("denied"))] {
        let at = i64::try_from(seq).expect("small") * 1_000;
        let detail = json!({ "tool": "bash", "level": "info" }).to_string();
        let hash = legacy_hash(&prev, seq, at, action, outcome, &detail);
        place.raw(
            "INSERT INTO audit_trail (seq, at, actor, subject, action, policy, outcome, detail, prev, hash)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            args![seq, at, "dev", "ana", action, "floor", outcome, detail, prev, hash],
        );
        prev = hash;
    }
    prev
}

fn versioned(at: i64) -> AuditEvent {
    AuditEvent {
        actor: Some("dev".into()),
        subject: Some("ana".into()),
        outcome: Some("success".into()),
        operation: Some("orders.approve@3".into()),
        friendly_name: Some("Approve order".into()),
        policy_version: Some("0123abcd".into()),
        decision_code: Some("approval.granted".into()),
        layer: Some("list".into()),
        approval_wait_ms: Some(1_500),
        authorized_by: Some(AuthorizedBy::Approval("policy-7".into())),
        creator_type: Some(CreatorType::Automation),
        agent: Some("dev@9f3a".into()),
        detail: json!({ "connection": "c-1" }),
        ..AuditEvent::new(at, "operation.call")
    }
}

fn old_and_new_records_verify_together_and_both_stay_readable(fresh: Fresh) {
    let place = fresh();
    let store = place.open(0);
    let last_legacy = legacy(&place);
    let mut blocked =
        AuditEvent { schema_version: 0, at: 4_000, action: "policy.blocked".into(), ..AuditEvent::default() };
    blocked.failure_stage = Some(FailureStage::Authorization);
    blocked.decision_code = Some("floor.denied".into());
    assert_eq!(store.append_audit(&[versioned(3_000), blocked]).expect("append"), 4);
    let verified = store.verify_audit().expect("verify");
    assert_eq!((verified.checked, verified.broken), (4, None));
    let all = store.audit(&AuditFilter::default(), AuditPage::After(0), 10).expect("all");
    assert_eq!(all[0].event.schema_version, 0);
    assert_eq!(all[0].event.operation, None);
    assert_eq!(all[1].event.outcome.as_deref(), Some("denied"));
    assert_eq!(all[1].event.detail["tool"], "bash");
    assert_eq!(all[2].prev, last_legacy);
    assert_eq!(all[2].event, versioned(3_000));
    assert_eq!(all[2].event.schema_version, SCHEMA_VERSION);
    assert_eq!(all[3].event.failure_stage, Some(FailureStage::Authorization));
    assert_eq!(all[3].event.schema_version, SCHEMA_VERSION, "a new record is always written in the current schema");
    let shown = serde_json::to_value(&all[2]).expect("shown");
    assert_eq!(shown["authorizedBy"], "approval:policy-7");
    assert_eq!(shown["approvalWaitMs"], 1_500);
    assert_eq!(shown["seq"], 3);
}

scenarios!(old_and_new_records_verify_together_and_both_stay_readable);

#[test]
fn a_versioned_field_changed_or_a_version_lowered_breaks_the_chain() {
    for change in ["decision_code = 'list.denied'", "authorized_by = 'standing'", "schema_version = 0"] {
        let place = Place::sqlite();
        let store = place.open(0);
        legacy(&place);
        store.append_audit(&[versioned(3_000), versioned(4_000)]).expect("append");
        place.raw("DROP TRIGGER audit_trail_only_grows", &[]);
        place.raw(&format!("UPDATE audit_trail SET {change} WHERE seq = 3"), &[]);
        assert_eq!(store.verify_audit().expect("verify").broken, Some(3), "{change}");
    }
}
