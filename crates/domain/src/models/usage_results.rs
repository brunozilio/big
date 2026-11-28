use std::collections::BTreeMap;

use crate::billing::Cost;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fate {
    Running,
    Delivered,
    Stopped,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Outcome {
    pub workflow: String,
    pub kind: String,
    pub fate: Fate,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Spread {
    pub min: f64,
    pub p50: f64,
    pub p90: f64,
    pub max: f64,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ResultUsage {
    pub workflow: String,
    pub kind: String,
    pub items: u64,
    pub results: u64,
    pub running: u64,
    pub stopped: u64,
    pub cost: Cost,
    pub stopped_cost: Cost,
    pub spread: Option<Spread>,
}

impl ResultUsage {
    pub fn per_result(&self) -> Option<f64> {
        (self.results > 0).then(|| self.cost.known_usd() / self.results as f64)
    }
}

#[derive(Default)]
struct Group {
    usage: ResultUsage,
    delivered: Vec<f64>,
}

impl Group {
    fn count(&mut self, fate: Fate, cost: Cost) {
        let usage = &mut self.usage;
        usage.items += 1;
        usage.cost.add(cost);
        match fate {
            Fate::Running => usage.running += 1,
            Fate::Delivered => {
                usage.results += 1;
                self.delivered.push(cost.known_usd());
            }
            Fate::Stopped => {
                usage.stopped += 1;
                usage.stopped_cost.add(cost);
            }
        }
    }

    fn finish(mut self) -> ResultUsage {
        self.usage.spread = spread(&mut self.delivered);
        self.usage
    }
}

fn rank(sorted: &[f64], share: f64) -> f64 {
    let at = (share * sorted.len() as f64).ceil() as usize;
    sorted.get(at.saturating_sub(1)).copied().unwrap_or_default()
}

fn spread(costs: &mut [f64]) -> Option<Spread> {
    costs.sort_by(f64::total_cmp);
    let (min, max) = (costs.first().copied()?, costs.last().copied()?);
    Some(Spread { min, p50: rank(costs, 0.5), p90: rank(costs, 0.9), max })
}

pub fn by_result(spent: &BTreeMap<String, Cost>, outcome: impl Fn(&str) -> Option<Outcome>) -> Vec<ResultUsage> {
    let mut groups: BTreeMap<(String, String), Group> = BTreeMap::new();
    for (item, cost) in spent {
        let Some(found) = outcome(item) else { continue };
        let group = groups.entry((found.workflow.clone(), found.kind.clone())).or_insert_with(|| Group {
            usage: ResultUsage { workflow: found.workflow, kind: found.kind, ..ResultUsage::default() },
            delivered: Vec::new(),
        });
        group.count(found.fate, *cost);
    }
    let mut rows: Vec<ResultUsage> = groups.into_values().map(Group::finish).collect();
    rows.sort_by(|a, b| b.cost.known_micros.cmp(&a.cost.known_micros).then(a.workflow.cmp(&b.workflow)));
    rows
}

#[cfg(test)]
#[path = "usage_results_tests.rs"]
mod tests;
