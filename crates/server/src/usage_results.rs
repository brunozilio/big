use std::collections::BTreeMap;

use domain::billing::Cost;
use domain::models::usage_results::{Fate, Outcome, ResultUsage, by_result};
use domain::workflow::{Ending, ItemId, Output, WorkItem};
use serde_json::{Value, json};

use crate::state::Shared;

pub fn outcome(item: &WorkItem, output: impl FnOnce() -> Option<Output>) -> Option<Outcome> {
    let kept = match &item.ending {
        Some(Ending::Finished { delivered: Some(delivered), .. }) => Some(delivered.clone()),
        _ => None,
    };
    let declared = || output().map(|output| (output.deliver(&item.steps, 0), output.kind));
    let (delivered, kind) = match kept {
        Some(delivered) => {
            let kind = delivered.kind.clone();
            (Some(delivered), kind)
        }
        None => declared()?,
    };
    let fate = match &item.ending {
        None => Fate::Running,
        Some(Ending::Finished { .. }) if delivered.is_some_and(|delivered| delivered.done) => Fate::Delivered,
        Some(_) => Fate::Stopped,
    };
    Some(Outcome { workflow: item.workflow.clone(), kind, fate })
}

fn row(usage: &ResultUsage) -> Value {
    let spread = usage
        .spread
        .map(|spread| json!({ "min": spread.min, "p50": spread.p50, "p90": spread.p90, "max": spread.max }));
    json!({
        "workflow": usage.workflow, "kind": usage.kind,
        "items": usage.items, "results": usage.results, "running": usage.running, "stopped": usage.stopped,
        "costUsd": usage.cost.known_usd(), "unpricedCalls": usage.cost.unpriced_calls,
        "costPerResult": usage.per_result(), "stoppedCostUsd": usage.stopped_cost.known_usd(),
        "spread": spread,
    })
}

pub fn rows(spent: &BTreeMap<String, Cost>, outcome_of: impl Fn(&str) -> Option<Outcome>) -> Value {
    json!(by_result(spent, outcome_of).iter().map(row).collect::<Vec<_>>())
}

pub fn by_result_of(shared: &Shared, spent: &BTreeMap<String, Cost>) -> Value {
    let orchestrator = &shared.backend.orchestrator;
    rows(spent, |id| {
        let item = orchestrator.item(&ItemId::from(id)).ok().flatten()?;
        outcome(&item, || orchestrator.output_of(&item))
    })
}

#[cfg(test)]
#[path = "usage_results_tests.rs"]
mod tests;
