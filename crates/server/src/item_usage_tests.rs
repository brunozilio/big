use super::*;
use domain::Compaction;
use domain::billing::ModelUsage;

fn call(input: u64, output: u64, cache_read: u64, cost: Option<f64>) -> EngineEvent {
    EngineEvent::Usage(ModelUsage {
        model: "claude-opus-5-5".into(),
        input,
        output,
        cache_read,
        estimated_cost_usd: cost,
        ..ModelUsage::default()
    })
}

#[test]
fn the_cost_adds_every_session_and_every_sub_agent() {
    let first = vec![call(100, 10, 0, Some(0.5))];
    let sub = EngineEvent::AgentEvent { index: 0, event: Box::new(call(50, 5, 0, Some(0.25))) };
    let second = vec![call(200, 20, 1000, Some(1.0)), sub];
    let (spent, _) = tally(&[first, second]);
    assert_eq!(spent.cost.known_micros, 1_750_000);
    assert!(spent.priced());
    assert_eq!((spent.input, spent.output, spent.cache_read, spent.calls), (350, 35, 1000, 3));
}

#[test]
fn an_unpriced_history_has_no_cost() {
    let (spent, _) = tally(&[vec![call(10, 1, 0, None)]]);
    assert_eq!(spent.json()["costUsd"], Value::Null);
    assert_eq!(spent.json()["unpricedCalls"], 1);
}

#[test]
fn an_item_with_one_call_without_a_price_shows_the_partial_and_the_count() {
    let calls = vec![call(100, 10, 0, Some(0.5)), call(100, 10, 0, Some(0.25)), call(100, 10, 0, None)];
    let (spent, _) = tally(&[calls]);
    let shown = spent.json();
    assert_eq!(shown["costUsd"], 0.75, "the partial of what is known");
    assert_eq!(shown["unpricedCalls"], 1);
    assert_eq!(shown["calls"], 3);
    let runs = [Ran { step: "dev".into(), agent: "a".into() }];
    let (steps, agents) = shares(&runs, &[vec![call(1, 1, 0, Some(0.5)), call(1, 1, 0, None)]]);
    assert_eq!((steps[0]["costUsd"].clone(), steps[0]["unpricedCalls"].clone()), (json!(0.5), json!(1)));
    assert_eq!(agents[0]["unpricedCalls"], 1);
}

#[test]
fn the_context_is_the_last_main_call_of_the_latest_session() {
    let older = vec![call(90_000, 1, 0, None)];
    let sub = EngineEvent::AgentEvent { index: 0, event: Box::new(call(5, 5, 0, None)) };
    let latest = vec![call(100, 10, 0, None), call(1000, 50, 20_000, None), sub];
    let (_, context) = tally(&[older, latest]);
    let context = context.expect("a measured context");
    assert_eq!(context.used, 21_050);
    assert_eq!(context.model.as_deref(), Some("claude-opus-5-5"));
}

#[test]
fn a_compaction_sets_the_context_to_what_it_left() {
    let compaction = Compaction { before: 120_000, after: 8_000, summary: String::new() };
    let events = vec![call(120_000, 0, 0, None), EngineEvent::Compacted(compaction)];
    let (_, context) = tally(&[events]);
    let context = context.expect("a measured context");
    assert_eq!(context.used, 8_000);
}

#[test]
fn an_item_without_calls_has_no_context() {
    assert_eq!(tally(&[]).1, None);
    assert_eq!(tally(&[vec![EngineEvent::Notice("oi".into())]]).1, None);
}

#[test]
fn the_share_of_the_window_is_bounded() {
    assert_eq!(percent(50_000, 200_000), 25);
    assert_eq!(percent(300_000, 200_000), 100);
    assert_eq!(percent(10, 0), 0);
}

#[test]
fn the_cost_is_split_by_step_and_by_agent() {
    let ran = |step: &str, agent: &str| Ran { step: step.into(), agent: agent.into() };
    let runs = [ran("plan", "planner"), ran("code", "dev"), ran("review", "dev")];
    let sessions = [vec![call(100, 10, 0, Some(0.5))], vec![call(200, 20, 0, Some(1.0))], vec![call(50, 5, 0, None)]];
    let (steps, agents) = shares(&runs, &sessions);
    let steps: Vec<(Value, Value, Value)> =
        steps.iter().map(|row| (row["step"].clone(), row["agent"].clone(), row["costUsd"].clone())).collect();
    assert_eq!(
        steps,
        [
            (json!("plan"), json!("planner"), json!(0.5)),
            (json!("code"), json!("dev"), json!(1.0)),
            (json!("review"), json!("dev"), Value::Null),
        ]
    );
    let agents: Vec<(Value, Value, Value)> =
        agents.iter().map(|row| (row["agent"].clone(), row["costUsd"].clone(), row["calls"].clone())).collect();
    assert_eq!(agents, [(json!("planner"), json!(0.5), json!(1)), (json!("dev"), json!(1.0), json!(2))]);
}
