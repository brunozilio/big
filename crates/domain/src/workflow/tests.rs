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
fn going_back_to_a_step_makes_the_steps_after_it_pending_again() {
    let item = replay(&[
        created(),
        started("develop"),
        finished("develop", StepState::Passed),
        started("checks"),
        finished("checks", StepState::Failed),
        reset("develop", "main", "pending"),
        reset("checks", "main", "pending"),
        started("develop"),
    ]);
    let states: Vec<(&str, StepState, u32)> =
        item.steps.iter().map(|run| (run.id.as_str(), run.state, run.attempt)).collect();
    assert_eq!(states, [("develop", StepState::Running, 2), ("checks", StepState::Pending, 1)]);
}

fn reset(step: &str, branch: &str, state: &str) -> WorkflowEvent {
    WorkflowEvent::Signal {
        seq: 0,
        kind: "step.reset".into(),
        branch: branch.into(),
        step: Some(step.into()),
        attempt: 1,
        data: json!({ "state": state }),
    }
}

fn lane(step: &str, branch: &str) -> WorkflowEvent {
    WorkflowEvent::StepStarted { step: step.into(), branch: Some(branch.into()) }
}

#[test]
fn a_reset_reaches_the_steps_inside_a_group_and_its_lanes_but_not_its_neighbours() {
    let item = replay(&[
        created(),
        lane("checks", "main"),
        lane("checks/lint", "main"),
        lane("checks/test", "main/checks/test.0"),
        lane("checks-extra", "main"),
        lane("checks/lint", "other"),
        reset("checks", "main", "skipped"),
    ]);
    let states: Vec<(&str, Option<&str>, StepState)> =
        item.steps.iter().map(|run| (run.id.as_str(), run.branch.as_deref(), run.state)).collect();
    assert_eq!(
        states,
        [
            ("checks", Some("main"), StepState::Skipped),
            ("checks/lint", Some("main"), StepState::Skipped),
            ("checks/test", Some("main/checks/test.0"), StepState::Skipped),
            ("checks-extra", Some("main"), StepState::Running),
            ("checks/lint", Some("other"), StepState::Running),
        ]
    );
}

#[test]
fn a_cancelling_reset_keeps_what_already_finished() {
    let item = replay(&[
        created(),
        lane("lint", "main"),
        WorkflowEvent::StepFinished {
            step: "lint".into(),
            branch: Some("main".into()),
            state: StepState::Passed,
            output: json!(1),
        },
        reset("lint", "main", "skipped"),
    ]);
    assert_eq!(item.step("lint", Some("main")).map(|run| run.state), Some(StepState::Passed));
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
        WorkflowEvent::Rewound { to: "b".into(), at: None },
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
fn a_named_item_keeps_its_name() {
    let item = replay(&[created(), WorkflowEvent::Named { title: "Loja virtual".into() }]);
    assert_eq!(item.name.as_deref(), Some("Loja virtual"));
}

#[test]
fn rewinding_clears_the_impediment_of_the_run_it_replaces() {
    let item = replay(&[
        created(),
        started("a"),
        finished("a", StepState::Failed),
        WorkflowEvent::ImpedimentSet { on: true, reason: Some("a falhou".into()) },
        WorkflowEvent::Rewound { to: "a".into(), at: None },
    ]);
    assert!(item.impediment.is_none());
}

#[test]
fn rewinding_to_an_unknown_step_changes_nothing_but_the_version() {
    let item = replay(&[created(), started("a")]);
    let rewound = reduce(item.clone(), &WorkflowEvent::Rewound { to: "zzz".into(), at: None });
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
    let asked = json!({"asks": {"a1": {"step": "s"}}, "answered": {"a0": {"note": 1}}});
    let item = reduce(item, &WorkflowEvent::EffectProgress { key: key("s", 1), data: asked });
    let closed = json!({"asks": {"a1": null}, "answered": {"a1": {"note": 2}}});
    let item = reduce(item, &WorkflowEvent::EffectProgress { key: key("s", 1), data: closed });
    assert_eq!(
        item.effects[&key("s", 1).to_string()].progress,
        json!({"session": "abc", "asks": {}, "answered": {"a0": {"note": 1}, "a1": {"note": 2}}})
    );
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

#[test]
fn the_first_branch_sticks_and_the_latest_recap_and_heads_up_per_step_win() {
    let note = |title: &str| HeadsUpNote { title: title.into(), explain: "porque".into(), urgent: false };
    let item = replay(&[
        created(),
        WorkflowEvent::Branched { name: "big/primeira".into() },
        WorkflowEvent::Branched { name: "big/segunda".into() },
        WorkflowEvent::Recapped { recap: ItemRecap { text: "antes".into(), after: "k1".into() } },
        WorkflowEvent::Recapped { recap: ItemRecap { text: "agora".into(), after: "k2".into() } },
        WorkflowEvent::HeadsUp { step: "develop".into(), note: note("velho") },
        WorkflowEvent::HeadsUp { step: "develop".into(), note: note("novo") },
        WorkflowEvent::HeadsUp { step: "review".into(), note: note("outro") },
    ]);
    assert_eq!(item.branch.as_deref(), Some("big/primeira"));
    assert_eq!(item.recap.as_ref().map(|recap| recap.text.as_str()), Some("agora"));
    assert_eq!(item.heads_up.get("develop").map(|note| note.title.as_str()), Some("novo"));
    assert_eq!(item.heads_up.len(), 2);
}

#[test]
fn a_linked_item_stays_linked_until_released_and_a_new_link_takes_it_back() {
    let member = ItemId::from("story-1");
    let item = replay(&[
        created(),
        WorkflowEvent::Accepting { on: true },
        WorkflowEvent::Linked { item: member.clone() },
        WorkflowEvent::Linked { item: member.clone() },
    ]);
    assert_eq!(item.links, vec![member.clone()]);
    assert!(item.accepting);
    let released = reduce(item, &WorkflowEvent::Unlinked { item: member.clone() });
    assert!(released.links.is_empty());
    assert_eq!(released.released, vec![member.clone()]);
    let back = reduce(released, &WorkflowEvent::Linked { item: member.clone() });
    assert_eq!((back.links.clone(), back.released.is_empty()), (vec![member], true));
    let closed = reduce(back, &WorkflowEvent::Accepting { on: false });
    assert!(!closed.accepting);
}

#[test]
fn a_chosen_base_branch_replaces_the_one_the_flow_gave_only_for_its_repository() {
    let item = replay(&[
        created(),
        WorkflowEvent::BaseChosen { repository: "web".into(), base: "main".into() },
        WorkflowEvent::BaseChosen { repository: "api".into(), base: "trunk".into() },
    ]);
    assert_eq!(item.input["git"]["web"]["base"], "main");
    assert_eq!(item.input["git"]["api"]["base"], "trunk");
}
