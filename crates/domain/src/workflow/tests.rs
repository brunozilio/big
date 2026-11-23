use std::time::Duration;

use serde_json::json;

use super::*;
use crate::effect::{Effect, EffectOutcome, EffectRequest, ErrorClass, IdempotencyKey};

fn created() -> WorkflowEvent {
    WorkflowEvent::ItemCreated {
        id: ItemId::from("item-1"),
        workflow: "any-flow".into(),
        parent: None,
        input: json!({"title": "x"}),
    }
}

fn started(step: &str) -> WorkflowEvent {
    WorkflowEvent::StepStarted { step: step.into(), branch: None }
}

fn key(step: &str, seq: u64) -> IdempotencyKey {
    IdempotencyKey::new("item-1", step, None, seq, 1)
}

fn requested(step: &str, seq: u64, effect: Effect) -> WorkflowEvent {
    WorkflowEvent::EffectRequested(EffectRequest::new(key(step, seq), effect))
}

fn resolved(step: &str, seq: u64, value: serde_json::Value) -> WorkflowEvent {
    WorkflowEvent::EffectResolved { key: key(step, seq), outcome: EffectOutcome::Ok { value } }
}

fn finished(step: &str, state: StepState) -> WorkflowEvent {
    WorkflowEvent::StepFinished { step: step.into(), branch: None, state, output: json!(null) }
}

fn exec() -> Effect {
    Effect::Exec { command: "true".into(), cwd: None }
}

#[test]
fn creating_an_item_fills_its_identity_and_nothing_else() {
    let item = replay(&[created()]);
    assert_eq!(item.id, ItemId::from("item-1"));
    assert_eq!(item.workflow, "any-flow");
    assert!(item.steps.is_empty() && item.status.is_none() && item.impediment.is_none() && item.ending.is_none());
    assert_eq!(item.version, 1);
}

#[test]
fn starting_a_step_runs_it_and_starting_again_counts_an_attempt() {
    let item =
        replay(&[created(), started("free-name"), finished("free-name", StepState::Failed), started("free-name")]);
    assert_eq!(item.steps.len(), 1);
    assert_eq!(item.steps[0].state, StepState::Running);
    assert_eq!(item.steps[0].attempt, 2);
}

#[test]
fn branches_of_one_step_are_separate_runs() {
    let item = replay(&[
        created(),
        WorkflowEvent::StepStarted { step: "par".into(), branch: Some("a".into()) },
        WorkflowEvent::StepStarted { step: "par".into(), branch: Some("b".into()) },
        WorkflowEvent::StepFinished {
            step: "par".into(),
            branch: Some("b".into()),
            state: StepState::Passed,
            output: json!(7),
        },
    ]);
    assert_eq!(item.step("par", Some("a")).map(|s| s.state), Some(StepState::Running));
    let b = item.step("par", Some("b"));
    assert_eq!(b.map(|s| s.state), Some(StepState::Passed));
    assert_eq!(b.map(|s| s.output.clone()), Some(json!(7)));
}

#[test]
fn requested_effects_stay_pending_until_resolved_and_then_answer_replay() {
    let item = replay(&[created(), started("s"), requested("s", 1, exec())]);
    assert_eq!(item.pending_effects().count(), 1);
    assert_eq!(item.outcome(&key("s", 1)), None);
    let item = reduce(item, &resolved("s", 1, json!({"code": 0})));
    assert_eq!(item.pending_effects().count(), 0);
    assert_eq!(item.outcome(&key("s", 1)), Some(&EffectOutcome::Ok { value: json!({"code": 0}) }));
}

#[test]
fn waiting_for_a_human_marks_the_step_and_shows_the_view_until_answered() {
    let view = json!([{"block": "text"}]);
    let item = replay(&[created(), started("s"), requested("s", 1, Effect::WaitHuman { view: view.clone() })]);
    assert_eq!(item.steps[0].state, StepState::WaitingHuman);
    assert_eq!(item.steps[0].view, view);
    assert!(item.waiting_human());
    let item = reduce(
        item,
        &WorkflowEvent::HumanInput { step: "s".into(), branch: None, action: "approve".into(), payload: json!({}) },
    );
    assert_eq!(item.steps[0].state, StepState::Running);
    assert_eq!(item.inputs.len(), 1);
    assert_eq!(item.inputs[0].action, "approve");
}

#[test]
fn resolving_a_human_wait_also_resumes_the_step() {
    let item = replay(&[
        created(),
        started("s"),
        requested("s", 1, Effect::WaitHuman { view: json!(null) }),
        resolved("s", 1, json!("ok")),
    ]);
    assert_eq!(item.steps[0].state, StepState::Running);
}

#[test]
fn status_is_any_string_the_workflow_chooses() {
    let item = replay(&[created(), WorkflowEvent::StatusSet("Whatever the client named it".into())]);
    assert_eq!(item.status.as_deref(), Some("Whatever the client named it"));
}

#[test]
fn impediments_come_and_go() {
    let on = WorkflowEvent::ImpedimentSet { on: true, reason: Some("retries spent".into()) };
    let item = replay(&[created(), on]);
    assert_eq!(item.impediment, Some(Impediment { reason: Some("retries spent".into()) }));
    let item = reduce(item, &WorkflowEvent::ImpedimentSet { on: false, reason: None });
    assert_eq!(item.impediment, None);
}

#[test]
fn children_are_linked_and_their_output_recorded() {
    let child = ItemId::from("child-1");
    let item = replay(&[
        created(),
        WorkflowEvent::ChildStarted { child: child.clone(), workflow: "scan".into() },
        WorkflowEvent::ChildFinished { child: child.clone(), output: json!({"ok": true}) },
    ]);
    assert_eq!(
        item.children,
        [ChildLink { id: child, workflow: "scan".into(), finished: true, output: json!({"ok": true}) }]
    );
}

#[test]
fn timers_are_pending_until_fired() {
    let item = replay(&[
        created(),
        WorkflowEvent::TimerSet { id: "t1".into(), at: 1_000 },
        WorkflowEvent::TimerSet { id: "t2".into(), at: 2_000 },
        WorkflowEvent::TimerFired { id: "t1".into() },
    ]);
    assert_eq!(item.timers.into_iter().collect::<Vec<_>>(), [("t2".to_owned(), 2_000)]);
}

#[test]
fn finishing_and_cancelling_end_the_item() {
    let done = replay(&[created(), WorkflowEvent::Finished { output: json!(1) }]);
    assert!(done.is_finished() && done.is_ended() && !done.is_cancelled());
    let cancelled = replay(&[created(), WorkflowEvent::Cancelled { reason: "by hand".into() }]);
    assert!(cancelled.is_cancelled() && cancelled.is_ended() && !cancelled.is_finished());
}

#[test]
fn rewinding_drops_what_came_after_the_step_and_resets_it() {
    let item = replay(&[
        created(),
        started("a"),
        requested("a", 1, exec()),
        resolved("a", 1, json!(1)),
        finished("a", StepState::Passed),
        started("b"),
        requested("b", 2, exec()),
        resolved("b", 2, json!(2)),
        finished("b", StepState::Passed),
        started("c"),
        requested("c", 3, exec()),
        WorkflowEvent::Finished { output: json!(null) },
        WorkflowEvent::Rewound { to: "b".into() },
    ]);
    let ids: Vec<_> = item.steps.iter().map(|s| (s.id.as_str(), s.state)).collect();
    assert_eq!(ids, [("a", StepState::Passed), ("b", StepState::Pending)]);
    assert_eq!(item.steps[1].attempt, 1);
    assert!(item.outcome(&key("a", 1)).is_some());
    assert!(item.outcome(&key("b", 2)).is_none());
    assert_eq!(item.effects.len(), 1);
    assert!(!item.is_ended());
    let again = reduce(item, &started("b"));
    assert_eq!(again.steps[1].attempt, 2);
}

#[test]
fn rewinding_to_an_unknown_step_changes_nothing_but_the_version() {
    let item = replay(&[created(), started("a")]);
    let rewound = reduce(item.clone(), &WorkflowEvent::Rewound { to: "zzz".into() });
    assert_eq!(rewound.steps, item.steps);
    assert_eq!(rewound.version, item.version + 1);
}

#[test]
fn a_failed_effect_is_resolved_too() {
    let item = replay(&[
        created(),
        started("s"),
        requested("s", 1, Effect::Sleep { duration: Duration::from_secs(1) }),
        WorkflowEvent::EffectResolved {
            key: key("s", 1),
            outcome: EffectOutcome::Failed { class: ErrorClass::Permanent, message: "no".into() },
        },
    ]);
    assert_eq!(item.pending_effects().count(), 0);
    assert!(item.outcome(&key("s", 1)).is_some_and(|o| !o.is_ok()));
}

#[test]
fn events_and_items_round_trip_through_json() {
    let events = [
        created(),
        started("s"),
        requested("s", 1, exec()),
        resolved("s", 1, json!(0)),
        WorkflowEvent::StatusSet("Doing".into()),
        WorkflowEvent::StepViewSet { step: "s".into(), branch: None, view: json!(["v"]) },
        finished("s", StepState::Skipped),
    ];
    for event in &events {
        let back: Option<WorkflowEvent> =
            serde_json::to_value(event).ok().and_then(|value| serde_json::from_value(value).ok());
        assert_eq!(back.as_ref(), Some(event));
    }
    let item = replay(&events);
    let back: Option<WorkItem> = serde_json::to_value(&item).ok().and_then(|value| serde_json::from_value(value).ok());
    assert_eq!(back, Some(item));
}

#[test]
fn step_states_are_written_in_snake_case() {
    assert_eq!(serde_json::to_value(StepState::WaitingHuman).ok(), Some(json!("waiting_human")));
}

#[test]
fn replay_is_the_fold_of_reduce() {
    let events = [created(), started("a"), requested("a", 1, exec()), finished("a", StepState::Passed)];
    let folded = events.iter().fold(WorkItem::default(), reduce);
    assert_eq!(replay(&events), folded);
    assert_eq!(folded.version, 4);
}

#[test]
fn progress_of_an_effect_is_kept_and_signals_change_nothing() {
    let signal = WorkflowEvent::Signal {
        seq: 0,
        kind: "fork".into(),
        branch: "main".into(),
        step: None,
        attempt: 0,
        data: json!({"branches": ["main/0.0"]}),
    };
    let item = replay(&[created(), signal, started("s"), requested("s", 1, exec())]);
    let before = item.clone();
    let item = reduce(item, &WorkflowEvent::EffectProgress { key: key("s", 1), data: json!({"session": "abc"}) });
    assert_eq!(item.effects[&key("s", 1).to_string()].progress, json!({"session": "abc"}));
    assert_eq!(item.version, before.version + 1);
    assert_eq!(item.steps, before.steps);
}

fn observed(id: &str, subject: &str, to: &str, at: i64) -> WorkflowEvent {
    WorkflowEvent::ExternalObserved(Observed {
        id: id.into(),
        connection: "c1".into(),
        kind: "tracker.status.changed".into(),
        subject: subject.into(),
        data: json!({"to": to}),
        at,
    })
}

#[test]
fn an_observed_external_event_keeps_the_newest_per_kind_and_subject_even_out_of_order() {
    let item = replay(&[created(), observed("b", "K-1", "Done", 20), observed("a", "K-1", "Doing", 10)]);
    assert_eq!(item.observed.len(), 1);
    assert_eq!(item.latest_observed("K-1").map(|seen| seen.id.as_str()), Some("b"));
    let item = reduce(item, &observed("c", "K-2", "Todo", 5));
    assert_eq!(item.observed.len(), 2);
    assert_eq!(item.latest_observed("K-2").map(|seen| seen.data["to"].clone()), Some(json!("Todo")));
    assert_eq!(item.version, 4);
}

#[test]
fn a_reconciliation_is_logged_without_touching_the_projection() {
    let before = replay(&[created(), WorkflowEvent::StatusSet("Doing".into())]);
    let reconciled = WorkflowEvent::Reconciled {
        field: "status".into(),
        local: json!("Doing"),
        remote: json!("Done"),
        action: "reapplied".into(),
    };
    let after = reduce(before.clone(), &reconciled);
    assert_eq!(after.version, before.version + 1);
    assert_eq!(WorkItem { version: before.version, ..after }, before);
    let back: Option<WorkflowEvent> =
        serde_json::to_value(&reconciled).ok().and_then(|value| serde_json::from_value(value).ok());
    assert_eq!(back, Some(reconciled));
}

#[test]
fn items_saved_before_observations_existed_still_load() {
    let mut value = serde_json::to_value(replay(&[created()])).unwrap_or_default();
    if let Some(fields) = value.as_object_mut() {
        fields.remove("observed");
    }
    let item: Option<WorkItem> = serde_json::from_value(value).ok();
    assert!(item.is_some_and(|item| item.observed.is_empty()));
}
