use std::collections::BTreeMap;

use super::*;
use crate::billing::Basis;

fn usd(value: f64) -> Cost {
    Cost::priced(Some(value), Basis::Api)
}

fn outcome(workflow: &str, fate: Fate) -> Option<Outcome> {
    Some(Outcome { workflow: workflow.into(), kind: "change".into(), fate })
}

#[test]
fn the_cost_per_result_spreads_every_item_of_the_workflow_over_the_delivered_ones() {
    let spent: BTreeMap<String, Cost> = [
        ("a".to_owned(), usd(1.0)),
        ("b".to_owned(), usd(3.0)),
        ("c".to_owned(), usd(2.0)),
        ("d".to_owned(), usd(0.5)),
        ("e".to_owned(), usd(9.0)),
        ("chat".to_owned(), usd(7.0)),
    ]
    .into();
    let rows = by_result(&spent, |item| match item {
        "a" | "b" | "c" => outcome("delivery", Fate::Delivered),
        "d" => outcome("delivery", Fate::Running),
        "e" => outcome("delivery", Fate::Stopped),
        _ => None,
    });
    assert_eq!(rows.len(), 1);
    let row = &rows[0];
    assert_eq!((row.workflow.as_str(), row.kind.as_str()), ("delivery", "change"));
    assert_eq!((row.items, row.results, row.running, row.stopped), (5, 3, 1, 1));
    assert_eq!(row.cost.known_usd(), 15.5);
    assert_eq!(row.stopped_cost.known_usd(), 9.0);
    assert_eq!(row.per_result().map(|cost| (cost * 1000.0).round() / 1000.0), Some(5.167));
    assert_eq!(row.spread, Some(Spread { min: 1.0, p50: 2.0, p90: 3.0, max: 3.0 }));
}

#[test]
fn a_workflow_that_spent_without_delivering_has_no_cost_per_result_and_rows_rank_by_cost() {
    let spent: BTreeMap<String, Cost> = [("a".to_owned(), usd(1.0)), ("b".to_owned(), usd(4.0))].into();
    let rows = by_result(&spent, |item| match item {
        "a" => outcome("small", Fate::Delivered),
        _ => outcome("big", Fate::Running),
    });
    assert_eq!(rows.iter().map(|row| row.workflow.as_str()).collect::<Vec<_>>(), ["big", "small"]);
    assert_eq!((rows[0].per_result(), rows[0].spread), (None, None));
    assert_eq!(rows[1].per_result(), Some(1.0));
    assert!(by_result(&BTreeMap::new(), |_| None).is_empty());
}

#[test]
fn the_report_keeps_the_spend_of_every_item_including_its_sub_agents() {
    use crate::EngineEvent;
    use crate::billing::ModelUsage;
    use crate::models::usage_report::UsageTally;
    use crate::origin::{Attribution, Origin};
    let day = "2026-11-23".to_owned();
    let charged = |item: &str, cost: f64| ModelUsage {
        attribution: Attribution::of_step(item, "develop", Origin::Step),
        estimated_cost_usd: Some(cost),
        ..ModelUsage::default()
    };
    let mut tally = UsageTally::new(std::slice::from_ref(&day), &[]);
    tally.add("s1", &day, &EngineEvent::Usage(charged("i1", 1.0)));
    let inner = EngineEvent::AgentEvent { index: 0, event: Box::new(EngineEvent::Usage(charged("i1", 0.5))) };
    tally.add("s1", &day, &inner);
    tally.add("s2", &day, &EngineEvent::Usage(charged("i2", 2.0)));
    tally.add("s3", &day, &EngineEvent::Usage(ModelUsage { estimated_cost_usd: Some(4.0), ..ModelUsage::default() }));
    tally.add("s2", "2026-11-01", &EngineEvent::Usage(charged("i2", 8.0)));
    let spent: Vec<(String, f64)> =
        tally.report().by_item.into_iter().map(|(item, cost)| (item, cost.known_usd())).collect();
    assert_eq!(spent, [("i1".to_owned(), 1.5), ("i2".to_owned(), 2.0)]);
}
