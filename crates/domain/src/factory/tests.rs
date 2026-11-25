use serde_json::{Value, json};

use super::*;

fn started(seq: u64, step: &str, kind: &str, options: Value) -> (i64, WorkflowEvent) {
    let data = json!({ "type": kind, "options": options });
    (
        0,
        WorkflowEvent::Signal {
            seq,
            kind: STEP_STARTED.into(),
            branch: "main".into(),
            step: Some(step.into()),
            attempt: 1,
            data,
        },
    )
}

fn finished(seq: u64, step: &str, attempt: u32, outcome: &str, data: Value, to: Option<&str>) -> (i64, WorkflowEvent) {
    let data = json!({ "outcome": outcome, "data": data, "to": to });
    let signal = WorkflowEvent::Signal {
        seq,
        kind: STEP_FINISHED.into(),
        branch: "main".into(),
        step: Some(step.into()),
        attempt,
        data,
    };
    (i64::try_from(seq).unwrap_or_default(), signal)
}

fn scan(events: &[(i64, WorkflowEvent)]) -> Vec<Finding> {
    let item = ItemId::from("item-1");
    let origin = Origin { item: &item, workflow: "entrega", generation: Some("g1") };
    findings(&origin, &["checks", "ci", "review"], events)
}

#[test]
fn a_step_that_passes_everywhere_leaves_no_finding() {
    let events = vec![
        started(0, "develop", "develop", json!({})),
        finished(1, "develop", 1, "passed", json!({ "branch": "b" }), None),
        started(2, "checks", "checks", json!({ "onFail": "develop" })),
        finished(3, "checks", 1, "passed", json!({}), None),
    ];
    assert!(scan(&events).is_empty());
}

#[test]
fn a_failed_step_is_a_failure_of_that_step_in_its_generation() {
    let events = vec![
        started(0, "develop", "develop", json!({})),
        finished(1, "develop", 1, "passed", json!({}), None),
        started(2, "checks", "checks", json!({ "onFail": "develop" })),
        finished(3, "checks", 1, "failed", json!({ "command": "cargo test" }), None),
    ];
    let found = scan(&events);
    assert_eq!(found.len(), 1, "checks catching develop right after it is not an escape");
    assert_eq!(found[0].kind, FindingKind::Failure);
    assert_eq!(found[0].step, "checks");
    assert_eq!(found[0].generation.as_deref(), Some("g1"));
    assert_eq!(found[0].detail["to"], "develop");
    assert_eq!(found[0].detail["data"]["command"], "cargo test");
}

#[test]
fn a_later_step_sending_work_back_past_a_gate_that_passed_is_an_escape_of_that_gate() {
    let events = vec![
        started(0, "develop", "develop", json!({})),
        finished(1, "develop", 1, "passed", json!({ "branch": "item/x" }), None),
        started(2, "checks", "checks", json!({ "commands": ["cargo test"], "onFail": "develop" })),
        finished(3, "checks", 1, "passed", json!({ "output": "ok" }), None),
        started(4, "pr", "pr", json!({})),
        finished(5, "pr", 1, "passed", json!({}), None),
        started(6, "review", "review", json!({ "onChanges": "develop" })),
        finished(7, "review", 1, "goto", json!({ "comments": [{ "body": "quebra o login" }] }), Some("develop")),
    ];
    let found = scan(&events);
    let escapes: Vec<&Finding> = found.iter().filter(|found| found.kind == FindingKind::Escape).collect();
    assert_eq!(escapes.len(), 1, "pr is not a gate, so only checks let it through");
    let escape = escapes[0];
    assert_eq!(escape.step, "develop");
    assert_eq!(escape.gate.as_deref(), Some("checks"));
    assert_eq!(escape.caught_by.as_deref(), Some("review"));
    assert_eq!(escape.detail["gate_options"]["commands"], json!(["cargo test"]));
    assert_eq!(escape.detail["producer_output"]["branch"], "item/x");
    assert_eq!(escape.detail["caught"]["comments"][0]["body"], "quebra o login");
    assert!(found.iter().any(|found| found.kind == FindingKind::Failure && found.step == "review"));
}

#[test]
fn a_human_rewinding_to_the_producer_after_the_gates_is_an_escape_caught_by_a_human() {
    let events = vec![
        started(0, "develop", "develop", json!({})),
        finished(1, "develop", 1, "passed", json!({}), None),
        started(2, "ci", "ci", json!({ "onFail": "develop" })),
        finished(3, "ci", 1, "passed", json!({}), None),
        (9, WorkflowEvent::Rewound { to: "develop".into(), at: None }),
    ];
    let found = scan(&events);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].kind, FindingKind::Escape);
    assert_eq!(found[0].gate.as_deref(), Some("ci"));
    assert_eq!(found[0].caught_by.as_deref(), Some(HUMAN));
    assert_eq!(found[0].at, 9);
}

#[test]
fn ids_are_stable_so_scanning_the_same_log_twice_finds_the_same_things() {
    let events = vec![
        started(0, "develop", "develop", json!({})),
        finished(1, "develop", 1, "passed", json!({}), None),
        started(2, "checks", "checks", json!({ "onFail": "develop" })),
        finished(3, "checks", 1, "passed", json!({}), None),
        started(4, "ci", "ci", json!({ "onFail": "develop" })),
        finished(5, "ci", 1, "failed", json!({}), None),
    ];
    let first: Vec<String> = scan(&events).into_iter().map(|found| found.id).collect();
    let second: Vec<String> = scan(&events).into_iter().map(|found| found.id).collect();
    assert_eq!(first, second);
    assert_eq!(first, vec!["item-1:ci:1:failure".to_owned(), "item-1:checks:1:escape:ci:1".to_owned()]);
}

#[test]
fn a_recurring_failure_crosses_the_threshold_at_each_multiple() {
    assert!(!crossed(0, 3));
    assert!(!crossed(2, 3));
    assert!(crossed(3, 3));
    assert!(!crossed(4, 3));
    assert!(crossed(6, 3));
    assert!(!crossed(5, 0));
}

#[test]
fn a_generation_depends_on_every_file_and_not_on_their_order() {
    let one = generation([("agents/a.md", "x"), ("workflows/w.ts", "y")]);
    let same = generation([("workflows/w.ts", "y"), ("agents/a.md", "x")]);
    let changed = generation([("agents/a.md", "x!"), ("workflows/w.ts", "y")]);
    assert_eq!(one, same);
    assert_ne!(one, changed);
    assert_eq!(one.len(), 16);
}

#[test]
fn a_step_that_fails_because_the_environment_broke_is_not_a_finding() {
    let events = vec![
        started(0, "develop", "develop", json!({})),
        finished(1, "develop", 1, "passed", json!({}), None),
        started(2, "checks", "checks", json!({ "onFail": "develop" })),
        finished(3, "checks", 1, "failed", json!({ "error": "timed out", "kind": "timeout" }), None),
        started(4, "plan", "plan", json!({})),
        finished(5, "plan", 1, "failed", json!({ "error": "not signed in", "kind": "permanent" }), None),
    ];
    assert!(scan(&events).is_empty(), "the factory cannot fix a timeout or a missing login");
}

fn failure(item: &str, step: &str, generation: &str, data: Value) -> Finding {
    Finding {
        id: format!("{item}:{step}:{generation}"),
        item: ItemId::from(item),
        workflow: "entrega".into(),
        generation: Some(generation.into()),
        kind: FindingKind::Failure,
        step: step.into(),
        gate: None,
        caught_by: None,
        detail: json!({ "data": data }),
        at: 0,
    }
}

#[test]
fn a_recurring_failure_counts_distinct_items_of_the_same_step_in_the_same_generation() {
    let wrong = json!({ "error": "reprovado" });
    let all = vec![
        failure("a", "checks", "g1", wrong.clone()),
        Finding { id: "a:checks:g1:2".into(), ..failure("a", "checks", "g1", wrong.clone()) },
        failure("b", "checks", "g1", wrong.clone()),
        failure("c", "checks", "g0", wrong.clone()),
        failure("d", "ci", "g1", wrong.clone()),
        failure("e", "checks", "g1", json!({ "error": "timed out", "kind": "timeout" })),
    ];
    let of = failure("b", "checks", "g1", wrong);
    let (count, same) = recurring(all, &of);
    assert_eq!(count, 2, "retries of one item count once and other generations do not count");
    assert_eq!(same.len(), 3);
}
