use domain::connection::{Capability, ConnectionId, ProviderKey};
use domain::effect::{EffectRequest, IdempotencyKey};
use domain::workflow::{Observed, StepState, WorkflowEvent, replay};
use serde_json::json;

use super::*;

fn created(input: Value) -> WorkflowEvent {
    WorkflowEvent::ItemCreated { id: ItemId::from("i1"), workflow: "flow".into(), parent: None, input }
}

fn call(seq: u64, action: &str, connection: Option<&str>, args: Value, value: Value) -> [WorkflowEvent; 2] {
    let key = IdempotencyKey::new("i1", "s", None, seq, 1);
    let effect = Effect::ConnectorCall(Call {
        capability: action.split('.').next().unwrap_or_default().into(),
        action: action.into(),
        connection: connection.map(str::to_owned),
        args,
        operation: None,
    });
    [
        WorkflowEvent::EffectRequested(EffectRequest::new(key.clone(), effect)),
        WorkflowEvent::EffectResolved { key, outcome: EffectOutcome::Ok { value } },
    ]
}

fn wait(seq: u64, kind: &str, filter: Value) -> WorkflowEvent {
    let key = IdempotencyKey::new("i1", "s", None, seq, 1);
    WorkflowEvent::EffectRequested(EffectRequest::new(
        key,
        Effect::WaitEvent { kind: kind.into(), filter, timeout: None },
    ))
}

fn connection(id: &str, capabilities: &[&str]) -> Connection {
    Connection {
        id: ConnectionId::from(id),
        provider: ProviderKey::from("p"),
        label: id.into(),
        account: String::new(),
        capabilities: capabilities.iter().map(|c| Capability::from(*c)).collect(),
    }
}

#[test]
fn the_issue_pull_request_and_run_come_from_effect_results_and_step_outputs() {
    let mut events = vec![created(json!({"title": "t"}))];
    events.extend(call(1, tracker::CREATE_ISSUE, Some("trk"), json!({"project": "P"}), json!({"ref": "P-1"})));
    events.extend(call(
        2,
        code::OPEN_PULL_REQUEST,
        Some("host"),
        json!({"head": "h"}),
        json!({"ref": "o/r#7", "state": "open"}),
    ));
    events.extend(call(3, pipelines::RUN, None, json!({"run": "o/r/runs/99"}), json!({"state": "running"})));
    events.push(WorkflowEvent::StepFinished {
        step: "s".into(),
        branch: None,
        state: StepState::Passed,
        output: json!({"pr": "o/r#8"}),
    });
    let desired = desired_of(&replay(&events), ReconcilePolicy::default());
    assert_eq!(desired.issue, Some(IssueRef { issue: "P-1".into(), connection: Some("trk".into()) }));
    assert_eq!(desired.pulls.len(), 2);
    assert_eq!(desired.pulls[0].state.as_deref(), Some("open"));
    assert_eq!(desired.pulls[0].connection.as_deref(), Some("host"));
    assert_eq!(desired.runs[0].state.as_deref(), Some("running"));
    assert_eq!(desired.subjects(), ["P-1", "o/r#7", "o/r#8", "o/r/runs/99"]);
    assert!(desired.uses("trk") && desired.uses("host") && !desired.uses("other"));
}

#[test]
fn the_input_issue_wins_and_events_seen_later_move_the_known_state() {
    let mut events = vec![created(json!({"issue": "K-9"}))];
    events.extend(call(1, code::MERGE, None, json!({"pr": "o/r#3"}), json!({"merged": false})));
    events.extend(call(2, pipelines::RUN, None, json!({"run": "o/r/runs/5"}), json!({"state": "queued"})));
    events.push(WorkflowEvent::ExternalObserved(Observed {
        id: "e".into(),
        connection: "host".into(),
        kind: kinds::PR_MERGED.into(),
        subject: "o/r#3".into(),
        data: json!({}),
        at: 10,
    }));
    let resolved_wait = IdempotencyKey::new("i1", "s", None, 3, 1);
    events.push(wait(3, kinds::RUN_FINISHED, json!({})));
    events.push(WorkflowEvent::EffectResolved {
        key: resolved_wait,
        outcome: EffectOutcome::Ok {
            value: json!({"kind": kinds::RUN_FINISHED, "subject": "o/r/runs/5", "data": {"state": "failed"}}),
        },
    });
    let desired = desired_of(&replay(&events), ReconcilePolicy::default());
    assert_eq!(desired.issue.map(|i| i.issue), Some("K-9".into()));
    assert_eq!(desired.pulls[0].state.as_deref(), Some(MERGED));
    assert!(desired.runs[0].finished());
    assert!(desired.waits.is_empty());
}

#[test]
fn a_pending_event_wait_makes_its_connection_hot() {
    let events = [created(json!({})), wait(1, "code.review.submitted", json!({"connection": "host"}))];
    let desired = desired_of(&replay(&events), ReconcilePolicy::default());
    assert!(desired.waits_on(&connection("host", &["code"])));
    assert!(!desired.waits_on(&connection("other", &["code"])));
    assert!(!desired.waits_on(&connection("host", &["tracker"])));
    assert!(desired.waits_for("code") && !desired.waits_for("pipelines"));
}

#[test]
fn the_sources_of_the_input_bring_the_issue_and_the_pull_requests() {
    let sources = json!([
        {"kind": "pull_request", "connection": "host", "subject": "o/r#4", "repository": "web"},
        {"kind": "issue", "connection": "trk", "subject": "K-2"},
        {"kind": "branch", "connection": null, "subject": "feature/x"},
    ]);
    let mut events = vec![created(json!({"sources": sources}))];
    events.extend(call(1, code::PULL_REQUEST, None, json!({"pr": "o/r#4"}), json!({"ref": "o/r#4", "state": "open"})));
    let desired = desired_of(&replay(&events), ReconcilePolicy::default());
    assert_eq!(desired.issue, Some(IssueRef { issue: "K-2".into(), connection: Some("trk".into()) }));
    assert_eq!(desired.pulls.len(), 1);
    assert_eq!(desired.pulls[0].state.as_deref(), Some("open"));
    assert_eq!(desired.pulls[0].connection.as_deref(), Some("host"));
    assert_eq!(desired.subjects(), ["K-2", "o/r#4"]);
    assert!(desired.uses("trk") && desired.uses("host"));
}

#[test]
fn the_legacy_issue_learns_its_connection_from_the_matching_source() {
    let input = json!({"issue": "K-9", "sources": [{"kind": "issue", "connection": "trk", "subject": "K-9"}]});
    let desired = desired_of(&replay(&[created(input)]), ReconcilePolicy::default());
    assert_eq!(desired.issue, Some(IssueRef { issue: "K-9".into(), connection: Some("trk".into()) }));
}
