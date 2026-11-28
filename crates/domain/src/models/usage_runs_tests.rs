use crate::EngineEvent;
use crate::billing::ModelUsage;
use crate::models::usage_report::UsageTally;
use crate::origin::{Attribution, Origin};

const DAY: &str = "2026-11-23";

fn window() -> Vec<String> {
    vec!["2026-11-22".to_owned(), DAY.to_owned()]
}

fn call(input: u64, read: u64, write: u64, cost: Option<f64>) -> ModelUsage {
    ModelUsage {
        model: "model-x".into(),
        input,
        output: 10,
        cache_read: read,
        cache_write: write,
        estimated_cost_usd: cost,
        provider: "provider-a".into(),
        ..ModelUsage::default()
    }
}

fn spawned(index: usize, kind: &str) -> EngineEvent {
    EngineEvent::AgentSpawned { index, kind: kind.into(), summary: format!("{kind} task"), prompt: String::new() }
}

fn inside(index: usize, usage: ModelUsage) -> EngineEvent {
    EngineEvent::AgentEvent { index, event: Box::new(EngineEvent::Usage(usage)) }
}

#[test]
fn each_run_splits_its_cost_between_the_lead_and_every_sub_agent() {
    let agents = [("s1".to_owned(), "dev".to_owned())].into();
    let mut tally = UsageTally::new(&window(), &[]).with_agents(agents);
    let owned =
        ModelUsage { attribution: Attribution::of_step("i1", "develop", Origin::Step), ..call(100, 900, 0, Some(1.0)) };
    tally.add("s1", DAY, &EngineEvent::Usage(owned));
    tally.add("s1", DAY, &spawned(0, "explore"));
    tally.add("s1", DAY, &spawned(1, "verify"));
    tally.add("s1", DAY, &inside(0, call(50, 0, 50, Some(0.25))));
    tally.add("s1", DAY, &inside(1, call(10, 90, 0, Some(0.5))));
    let nested = EngineEvent::AgentEvent { index: 1, event: Box::new(inside(4, call(10, 0, 0, None))) };
    tally.add("s1", DAY, &nested);
    tally.add("s2", DAY, &EngineEvent::Usage(call(10, 0, 0, Some(0.1))));
    let runs = tally.report().by_run;
    let order: Vec<&str> = runs.iter().map(|run| run.session.as_str()).collect();
    assert_eq!(order, ["s1", "s2"]);
    let run = &runs[0];
    assert_eq!(
        (run.agent.as_deref(), run.item.as_deref(), run.step.as_deref()),
        (Some("dev"), Some("i1"), Some("develop"))
    );
    assert_eq!((run.tally.calls, run.tally.cost.known_micros, run.tally.cost.unpriced_calls), (4, 1_750_000, 1));
    assert_eq!((run.lead.calls, run.lead.cost.known_micros), (1, 1_000_000));
    let lanes: Vec<(usize, &str, u64, u64)> = run
        .subagents
        .iter()
        .map(|lane| (lane.index, lane.kind.as_str(), lane.tally.calls, lane.tally.cost.known_micros))
        .collect();
    assert_eq!(lanes, [(1, "verify", 2, 500_000), (0, "explore", 1, 250_000)]);
    assert_eq!(run.subagents[0].summary, "verify task");
    assert_eq!(run.lead.cache_hit_rate(), Some(0.9));
    assert_eq!(run.subagents[1].tally.cache_hit_rate(), Some(0.0));
    assert_eq!(runs[1].agent, None);
}

#[test]
fn a_run_with_no_use_in_the_window_is_left_out_and_an_unannounced_lane_has_no_kind() {
    let mut tally = UsageTally::new(&window(), &[]);
    tally.add("old", "2026-11-01", &EngineEvent::Usage(call(10, 0, 0, Some(1.0))));
    tally.add("old", DAY, &spawned(0, "explore"));
    tally.add("s1", DAY, &inside(3, call(10, 0, 0, Some(1.0))));
    let runs = tally.report().by_run;
    assert_eq!(runs.len(), 1);
    assert_eq!((runs[0].subagents[0].index, runs[0].subagents[0].kind.as_str()), (3, ""));
    assert_eq!(runs[0].lead.calls, 0);
    assert_eq!(runs[0].lead.cache_hit_rate(), None);
}

#[test]
fn only_the_most_expensive_runs_are_kept() {
    let mut tally = UsageTally::new(&window(), &[]);
    for n in 0..super::SHOWN + 5 {
        tally.add(&format!("s{n:02}"), DAY, &EngineEvent::Usage(call(10, 0, 0, Some(n as f64))));
    }
    let runs = tally.report().by_run;
    assert_eq!(runs.len(), super::SHOWN);
    assert_eq!(runs[0].session, format!("s{:02}", super::SHOWN + 4));
}

#[test]
fn the_totals_tell_the_share_of_the_prompt_read_from_cache() {
    let mut tally = UsageTally::new(&window(), &[]);
    assert_eq!(tally.clone().report().totals.cache_hit_rate(), None);
    tally.add("s1", DAY, &EngineEvent::Usage(call(100, 600, 300, Some(1.0))));
    let report = tally.report();
    assert_eq!(report.totals.cache_hit_rate(), Some(0.6));
    assert_eq!(report.by_model[0].tally.cache_hit_rate(), Some(0.6));
}
