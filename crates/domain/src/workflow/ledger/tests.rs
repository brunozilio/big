use serde_json::json;

use super::*;
use crate::workflow::{ItemId, StepState, WorkflowEvent, replay};

fn started(step: &str) -> WorkflowEvent {
    WorkflowEvent::StepStarted { step: step.into(), branch: None }
}

fn finished(step: &str, state: StepState) -> WorkflowEvent {
    WorkflowEvent::StepFinished { step: step.into(), branch: None, state, output: json!(null) }
}

fn timed(events: Vec<(i64, WorkflowEvent)>) -> Vec<(u64, i64, WorkflowEvent)> {
    events.into_iter().enumerate().map(|(seq, (at, event))| (seq as u64 + 1, at, event)).collect()
}

fn created() -> WorkflowEvent {
    WorkflowEvent::ItemCreated {
        id: ItemId::from("i1"),
        workflow: "feature".into(),
        parent: None,
        input: json!({ "text": "Corrigir o login\ncom detalhes", "owner": "u-ana" }),
    }
}

fn line(item: &str, step: &str, micros: u64, unpriced: u64) -> SpendLine {
    SpendLine { item: item.into(), step: step.into(), micros, unpriced }
}

#[test]
fn the_checkpoint_says_what_was_done_in_the_window_and_what_is_still_open() {
    let events = timed(vec![
        (10, created()),
        (11, started("plan")),
        (20, finished("plan", StepState::Passed)),
        (110, started("develop")),
        (150, finished("develop", StepState::Passed)),
        (160, started("review")),
        (170, WorkflowEvent::ImpedimentSet { on: true, reason: Some("sem acesso".into()) }),
    ]);
    let all: Vec<WorkflowEvent> = events.iter().map(|(_, _, event)| event.clone()).collect();
    let item = replay(&all);
    let spend = [line("i1", "develop", 250_000, 0), line("i2", "develop", 999_000, 0), line("i1", "review", 0, 1)];
    let view = item_view(&item, "u-ana", &events, Window { since_ms: 100, until_ms: 200 }, &spend);
    assert_eq!(view.title, "Corrigir o login");
    assert_eq!(view.owner, "u-ana");
    assert_eq!(view.created_ms, Some(10));
    assert_eq!(view.updated_ms, Some(170));
    assert_eq!(
        view.checkpoint.work_done,
        [Done::Step { step: "develop".into(), branch: None, state: StepState::Passed, at_ms: 150 }],
        "the plan finished before the window"
    );
    assert_eq!(
        view.checkpoint.next_steps,
        [Next::Running { step: "review".into(), branch: None }, Next::Impediment { reason: Some("sem acesso".into()) }]
    );
    let develop = view.steps.iter().find(|step| step.id == "develop").expect("develop");
    assert_eq!((develop.started_ms, develop.finished_ms), (Some(110), Some(150)));
    let review = view.steps.iter().find(|step| step.id == "review").expect("review");
    assert_eq!((review.started_ms, review.finished_ms), (Some(160), None));
    assert_eq!(view.cost, Spend { usd: 0.25, unpriced_calls: 1 });
    let shown = serde_json::to_value(&view).expect("json");
    assert_eq!(shown["checkpoint"]["work_done"][0]["kind"], "step");
    assert_eq!(shown["checkpoint"]["next_steps"][1]["kind"], "impediment");
    assert_eq!(shown["group"], "with_you");
}

#[test]
fn an_ended_item_has_nothing_left_to_do() {
    let events = timed(vec![
        (10, created()),
        (11, started("plan")),
        (12, finished("plan", StepState::Passed)),
        (13, WorkflowEvent::Finished { output: json!({}), delivered: None }),
    ]);
    let all: Vec<WorkflowEvent> = events.iter().map(|(_, _, event)| event.clone()).collect();
    let view = item_view(&replay(&all), "local", &events, Window { since_ms: 0, until_ms: 100 }, &[]);
    assert!(view.ended);
    assert!(view.checkpoint.next_steps.is_empty());
    assert_eq!(view.checkpoint.work_done.last(), Some(&Done::Finished { at_ms: 13 }));
}

#[test]
fn the_cost_view_adds_up_per_item_and_step_and_keeps_conversations_apart() {
    let lines = [
        line("i1", "plan", 100_000, 0),
        line("i1", "develop", 400_000, 1),
        line("i2", "develop", 900_000, 0),
        line("", "", 50_000, 0),
    ];
    let view = cost_view(Some("u-ana".into()), Window { since_ms: 0, until_ms: 10 }, &lines, &|id| format!("t-{id}"));
    assert_eq!(view.total, Spend { usd: 1.45, unpriced_calls: 1 });
    assert_eq!(view.conversations, Spend { usd: 0.05, unpriced_calls: 0 });
    let order: Vec<&str> = view.items.iter().map(|item| item.id.as_str()).collect();
    assert_eq!(order, ["i2", "i1"], "the most expensive first");
    assert_eq!(view.items[1].title, "t-i1");
    let steps: Vec<(&str, f64)> = view.items[1].steps.iter().map(|step| (step.step.as_str(), step.spend.usd)).collect();
    assert_eq!(steps, [("develop", 0.4), ("plan", 0.1)]);
    let shown = serde_json::to_value(&view).expect("json");
    assert_eq!(shown["usd"], 1.45);
    assert_eq!(shown["since_ms"], 0);
}

#[test]
fn the_free_text_of_a_view_goes_through_the_cleaner() {
    let events = timed(vec![
        (10, created()),
        (11, WorkflowEvent::StatusSet("token sk-1".into())),
        (12, WorkflowEvent::ImpedimentSet { on: true, reason: Some("sk-2 expirou".into()) }),
        (13, WorkflowEvent::Cancelled { reason: "sk-3 vazou".into() }),
    ]);
    let all: Vec<WorkflowEvent> = events.iter().map(|(_, _, event)| event.clone()).collect();
    let mut item = replay(&all);
    item.ending = None;
    item.recap = Some(crate::workflow::ItemRecap { text: "usei sk-4".into(), after: String::new() });
    let view = item_view(&item, "u-ana", &events, Window { since_ms: 0, until_ms: 100 }, &[]);
    let clean = view.redacted(&|text: &str| text.replace("sk-", "[redacted]-"));
    let shown = serde_json::to_string(&clean).expect("json");
    assert!(!shown.contains("sk-"), "{shown}");
    assert!(shown.contains("[redacted]-2 expirou"), "{shown}");
}
