#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use connectors::contracts::{pipelines, tracker};
use domain::workflow::WorkflowEvent;
use serde_json::json;
use support::{HOST_CONNECTION, call, harness, standard_item, wait};
use sync::{FieldPolicy, ReconcilePolicy, desired_of};

fn pull_state(h: &support::Harness) -> Option<String> {
    desired_of(&h.item("i1"), ReconcilePolicy::default()).pulls[0].state.clone()
}

#[tokio::test]
async fn nothing_diverges_nothing_is_written() {
    let h = harness().await;
    standard_item(&h, "i1");
    let report = h.sync.reconcile_now().await.unwrap();
    assert_eq!((report.items, report.corrections), (1, 0));
    assert!(h.sink.recorded().is_empty() && h.fake.writes().is_empty());
}

#[tokio::test]
async fn lost_events_are_caught_and_without_a_rule_the_local_status_is_reapplied() {
    let h = harness().await;
    standard_item(&h, "i1");
    h.fake.world().issues.insert("K-1".into(), ("Done".into(), false));
    h.fake.world().pulls.insert("o/r#7".into(), "merged".into());
    let report = h.sync.reconcile_now().await.unwrap();
    assert_eq!(report.corrections, 2, "{report:?}");
    let writes = h.fake.writes();
    assert_eq!(writes.len(), 1);
    assert_eq!(writes[0].0, tracker::TRANSITION);
    assert_eq!(writes[0].1, json!({"issue": "K-1", "status": "Doing"}));
    assert_eq!(writes[0].2, "i1/reconcile/status");
    assert_eq!(h.fake.world().issues["K-1"].0, "Doing");
    assert_eq!(pull_state(&h).as_deref(), Some("merged"));
    let mut actions = h.sink.reconciled();
    actions.sort();
    assert_eq!(actions, [("pull_request".into(), "observed".into()), ("status".into(), "reapplied".into())]);
    let again = h.sync.reconcile_now().await.unwrap();
    assert_eq!(again.corrections, 0);
    assert_eq!(h.fake.writes().len(), 1);
}

#[tokio::test]
async fn a_status_changed_by_a_person_goes_to_the_workflow_when_a_rule_listens() {
    let h = harness().await;
    standard_item(&h, "i1");
    h.sink.trigger_on("tracker.status.changed");
    h.fake.world().issues.insert("K-1".into(), ("Blocked".into(), false));
    let report = h.sync.reconcile_now().await.unwrap();
    assert_eq!(report.corrections, 1);
    let triggered = h.sink.triggered();
    assert_eq!(triggered.len(), 1);
    assert_eq!(triggered[0].subject, "K-1");
    assert_eq!(triggered[0].data["from"], "Doing");
    assert_eq!(triggered[0].data["to"], "Blocked");
    assert!(h.fake.writes().is_empty());
    assert_eq!(h.sink.reconciled(), [("status".into(), "event".into())]);
    assert_eq!(h.sync.reconcile_now().await.unwrap().corrections, 0);
    assert_eq!(h.sink.triggered().len(), 1);
}

#[tokio::test]
async fn the_workflow_policy_can_adopt_the_remote_status_and_the_impediment_stays_local() {
    let h = harness().await;
    standard_item(&h, "i1");
    h.view.set_policy("flow", ReconcilePolicy { status: FieldPolicy::Remote, ..ReconcilePolicy::default() });
    h.fake.world().issues.insert("K-1".into(), ("Done".into(), true));
    let report = h.sync.reconcile_now().await.unwrap();
    assert_eq!(report.corrections, 2);
    assert_eq!(h.item("i1").status.as_deref(), Some("Done"));
    let writes = h.fake.writes();
    assert_eq!(writes.len(), 1);
    assert_eq!(writes[0].0, tracker::SET_IMPEDIMENT);
    assert_eq!(writes[0].1, json!({"issue": "K-1", "on": false}));
    assert!(!h.fake.world().issues["K-1"].1);
    let mut actions = h.sink.reconciled();
    actions.sort();
    assert_eq!(actions, [("impediment".into(), "reapplied".into()), ("status".into(), "adopted".into())]);
}

#[tokio::test]
async fn a_local_impediment_lost_remotely_is_put_back_with_its_reason() {
    let h = harness().await;
    standard_item(&h, "i1");
    h.add("i1", &[WorkflowEvent::ImpedimentSet { on: true, reason: Some("deploy rejected".into()) }]);
    h.sync.reconcile_now().await.unwrap();
    let writes = h.fake.writes();
    assert_eq!(writes[0].1, json!({"issue": "K-1", "on": true, "reason": "deploy rejected"}));
}

#[tokio::test]
async fn a_pull_request_closed_outside_wakes_whoever_waits_for_it() {
    let h = harness().await;
    standard_item(&h, "i1");
    h.sink.wait_on("o/r#7");
    h.fake.world().pulls.insert("o/r#7".into(), "closed".into());
    h.sync.reconcile_now().await.unwrap();
    let delivered = h.sink.delivered();
    assert_eq!(delivered.len(), 1);
    assert_eq!(delivered[0].kind, "code.pr.closed");
    assert_eq!(delivered[0].connection.as_str(), HOST_CONNECTION);
    assert_eq!(h.sink.reconciled(), [("pull_request".into(), "event".into())]);
    h.sync.reconcile_now().await.unwrap();
    assert_eq!(h.sink.delivered().len(), 1);
}

#[tokio::test]
async fn a_finished_run_resolves_the_wait_even_if_its_event_was_lost() {
    let h = harness().await;
    standard_item(&h, "i1");
    let mut events =
        call("i1", 2, pipelines::RUN, HOST_CONNECTION, json!({"run": "o/r/runs/42"}), json!({"state": "running"}))
            .to_vec();
    events.push(wait("i1", 3, "pipelines.run.finished", json!({"subject": "o/r/runs/42"})));
    h.add("i1", &events);
    h.sink.wait_on("o/r/runs/42");
    assert_eq!(h.sync.reconcile_now().await.unwrap().corrections, 0);
    h.fake.world().runs.insert("o/r/runs/42".into(), "passed".into());
    assert_eq!(h.sync.reconcile_now().await.unwrap().corrections, 1);
    let delivered = h.sink.delivered();
    assert_eq!(delivered.len(), 1);
    assert_eq!(delivered[0].kind, "pipelines.run.finished");
    assert_eq!(delivered[0].data["state"], "passed");
    assert_eq!(delivered[0].data["run"], "o/r/runs/42");
}

#[tokio::test]
async fn runs_are_left_alone_when_nothing_waits_for_a_pipeline() {
    let h = harness().await;
    standard_item(&h, "i1");
    let events =
        call("i1", 2, pipelines::RUN, HOST_CONNECTION, json!({"run": "o/r/runs/42"}), json!({"state": "running"}));
    h.add("i1", &events);
    h.fake.world().runs.insert("o/r/runs/42".into(), "passed".into());
    h.sync.reconcile_now().await.unwrap();
    let reads = h.fake.world().calls.iter().filter(|(action, _, _)| action == pipelines::RUN).count();
    assert_eq!(reads, 0);
}

#[tokio::test]
async fn ended_items_are_not_reconciled() {
    let h = harness().await;
    standard_item(&h, "i1");
    h.add("i1", &[WorkflowEvent::Finished { output: json!(null) }]);
    h.fake.world().issues.insert("K-1".into(), ("Done".into(), false));
    assert_eq!(h.sync.reconcile_now().await.unwrap().items, 0);
}
