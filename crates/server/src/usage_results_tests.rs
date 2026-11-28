use domain::billing::Basis;
use domain::workflow::{Audience, Delivered};

use super::*;
use crate::view_fixtures::{finished, ran};

fn declared() -> Option<Output> {
    Output::read(&json!({ "kind": "change", "key": "ref", "done": { "merged": true } })).ok().flatten()
}

fn kept(done: bool) -> Delivered {
    Delivered {
        kind: "change".into(),
        key: "o/r#7".into(),
        url: None,
        done,
        hash: "ab".into(),
        at: 1,
        audience: Audience::Me,
    }
}

fn of(workflow: &str, item: WorkItem) -> WorkItem {
    WorkItem { workflow: workflow.into(), ..item }
}

#[test]
fn a_finished_item_counts_as_a_result_only_when_its_deliverable_is_done() {
    let fate = |item: WorkItem| outcome(&item, declared).map(|found| found.fate);
    assert_eq!(fate(finished(Vec::new(), Some(kept(true)))), Some(Fate::Delivered));
    assert_eq!(fate(finished(Vec::new(), Some(kept(false)))), Some(Fate::Stopped));
    let merged = vec![ran("pr", json!({ "ref": "o/r#7" })), ran("merge", json!({ "ref": "o/r#7", "merged": true }))];
    assert_eq!(fate(finished(merged, None)), Some(Fate::Delivered));
    assert_eq!(fate(WorkItem::default()), Some(Fate::Running));
    let cancelled = WorkItem { ending: Some(Ending::Cancelled { reason: "x".into() }), ..WorkItem::default() };
    assert_eq!(fate(cancelled), Some(Fate::Stopped));
    assert_eq!(outcome(&WorkItem::default(), || None), None);
    assert_eq!(
        outcome(&finished(Vec::new(), Some(kept(true))), || None).map(|found| found.kind),
        Some("change".into())
    );
}

#[test]
fn each_workflow_with_a_declared_deliverable_shows_its_cost_per_result_and_spread() {
    let usd = |value: f64| Cost::priced(Some(value), Basis::Api);
    let spent: BTreeMap<String, Cost> = [
        ("a".to_owned(), usd(2.0)),
        ("b".to_owned(), usd(4.0)),
        ("c".to_owned(), usd(3.0)),
        ("chat".to_owned(), usd(9.0)),
    ]
    .into();
    let items: BTreeMap<&str, WorkItem> = [
        ("a", of("default", finished(Vec::new(), Some(kept(true))))),
        ("b", of("default", finished(Vec::new(), Some(kept(true))))),
        (
            "c",
            of("default", WorkItem { ending: Some(Ending::Cancelled { reason: "x".into() }), ..WorkItem::default() }),
        ),
    ]
    .into();
    let shown = rows(&spent, |id| items.get(id).and_then(|item| outcome(item, declared)));
    assert_eq!(
        shown,
        json!([{
            "workflow": "default", "kind": "change",
            "items": 3, "results": 2, "running": 0, "stopped": 1,
            "costUsd": 9.0, "unpricedCalls": 0, "costPerResult": 4.5, "stoppedCostUsd": 3.0,
            "spread": { "min": 2.0, "p50": 2.0, "p90": 4.0, "max": 4.0 },
        }])
    );
    assert_eq!(rows(&BTreeMap::new(), |_| None), json!([]));
}
