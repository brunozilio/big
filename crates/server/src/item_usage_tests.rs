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
    assert!((spent.cost_usd - 1.75).abs() < 1e-9);
    assert!(spent.priced);
    assert_eq!((spent.input, spent.output, spent.cache_read, spent.calls), (350, 35, 1000, 3));
}

#[test]
fn an_unpriced_history_has_no_cost() {
    let (spent, _) = tally(&[vec![call(10, 1, 0, None)]]);
    assert_eq!(spent.json()["costUsd"], Value::Null);
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
