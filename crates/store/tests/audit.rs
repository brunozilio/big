#![allow(clippy::expect_used, reason = "a failed setup step stops the test")]

#[macro_use]
mod common;

use std::time::Duration;

use common::{Fresh, Place};
use database::args;
use serde_json::json;
use domain::audit::AuditEvent;
use store::{AuditFilter, AuditPage, GENESIS};

fn draft(at: i64, actor: &str, subject: &str, action: &str) -> AuditEvent {
    AuditEvent {
        actor: Some(actor.into()),
        subject: Some(subject.into()),
        policy: Some("budgets".into()),
        outcome: Some("denied".into()),
        detail: json!({ "tool": "bash", "item": "i-1" }),
        ..AuditEvent::new(at, action)
    }
}

fn filled(place: &Place) -> store::Store {
    let store = place.open(0);
    let drafts = [
        draft(1_000, "dev", "ana", "policy.blocked"),
        draft(2_000, "review", "ana", "tool.call"),
        draft(3_000, "dev", "bia", "tool.call"),
        draft(4_000, "dev", "ana", "budget.overrun"),
    ];
    assert_eq!(store.append_audit(&drafts[..2]).expect("first"), 2);
    assert_eq!(store.append_audit(&drafts[2..]).expect("second"), 4);
    store
}

fn each_record_carries_the_hash_of_the_one_before(fresh: Fresh) {
    let place = fresh();
    let store = filled(&place);
    let all = store.audit(&AuditFilter::default(), AuditPage::After(0), 10).expect("all");
    assert_eq!(all.iter().map(|record| record.seq).collect::<Vec<_>>(), [1, 2, 3, 4]);
    assert_eq!(all[0].prev, GENESIS);
    for pair in all.windows(2) {
        assert_eq!(pair[1].prev, pair[0].hash);
    }
    assert_eq!(all[0].event.detail["tool"], "bash");
    let verified = store.verify_audit().expect("verify");
    assert_eq!((verified.checked, verified.broken, verified.last), (4, None, Some(4)));
}

fn the_trail_is_filtered_by_agent_person_action_and_period_and_paged(fresh: Fresh) {
    let place = fresh();
    let store = filled(&place);
    let dev = AuditFilter { actor: Some("dev".into()), ..AuditFilter::default() };
    let seqs = |filter: &AuditFilter, page, limit| {
        store.audit(filter, page, limit).expect("query").iter().map(|record| record.seq).collect::<Vec<_>>()
    };
    assert_eq!(seqs(&dev, AuditPage::Before(None), 10), [4, 3, 1]);
    assert_eq!(seqs(&dev, AuditPage::Before(None), 2), [4, 3]);
    assert_eq!(seqs(&dev, AuditPage::Before(Some(3)), 2), [1]);
    let period = AuditFilter { from: Some(2_000), to: Some(4_000), ..dev.clone() };
    assert_eq!(seqs(&period, AuditPage::Before(None), 10), [3]);
    let ana = AuditFilter { subject: Some("ana".into()), action: Some("tool.call".into()), ..AuditFilter::default() };
    assert_eq!(seqs(&ana, AuditPage::Before(None), 10), [2]);
    let facets = store.audit_facets().expect("facets");
    assert_eq!(facets.actors, ["dev", "review"]);
    assert_eq!(facets.subjects, ["ana", "bia"]);
}

fn a_record_cannot_be_changed_and_a_removed_one_breaks_the_chain(fresh: Fresh) {
    let place = fresh();
    let store = filled(&place);
    let changed = place.database().write(|conn| conn.execute("UPDATE audit_trail SET actor = 'x' WHERE seq = 2", &[]));
    assert!(changed.is_err(), "{changed:?}");
    place.raw("DELETE FROM audit_trail WHERE seq = ?1", args![2]);
    let verified = store.verify_audit().expect("verify");
    assert_eq!(verified.broken, Some(3));
}

fn retention_only_forgets_what_is_older_and_the_rest_still_verifies(fresh: Fresh) {
    let place = fresh();
    let store = filled(&place);
    assert_eq!(store.forget_audit(Duration::from_secs(100 * 365 * 86_400)).expect("keep"), 0);
    let now = jiff::Timestamp::now().as_millisecond();
    store.append_audit(&[draft(now, "dev", "ana", "tool.call")]).expect("recent");
    assert_eq!(store.forget_audit(Duration::from_secs(3600)).expect("forget"), 4);
    let verified = store.verify_audit().expect("verify");
    assert_eq!((verified.checked, verified.first, verified.broken), (1, Some(5), None));
}

scenarios!(
    each_record_carries_the_hash_of_the_one_before,
    the_trail_is_filtered_by_agent_person_action_and_period_and_paged,
    a_record_cannot_be_changed_and_a_removed_one_breaks_the_chain,
    retention_only_forgets_what_is_older_and_the_rest_still_verifies,
);

#[test]
fn a_record_changed_behind_the_guard_fails_its_hash() {
    let place = Place::sqlite();
    let store = filled(&place);
    place.raw("DROP TRIGGER audit_trail_only_grows", &[]);
    place.raw("UPDATE audit_trail SET outcome = 'allowed' WHERE seq = 2", &[]);
    assert_eq!(store.verify_audit().expect("verify").broken, Some(2));
}
