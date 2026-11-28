use std::collections::{BTreeMap, HashMap};

use crate::billing::ModelUsage;
use crate::models::usage_report::{Tally, ranking};

pub const SHOWN: usize = 20;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct SubagentUsage {
    pub index: usize,
    pub kind: String,
    pub summary: String,
    pub tally: Tally,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct RunUsage {
    pub session: String,
    pub agent: Option<String>,
    pub item: Option<String>,
    pub step: Option<String>,
    pub tally: Tally,
    pub lead: Tally,
    pub subagents: Vec<SubagentUsage>,
}

#[derive(Debug, Clone, Default)]
pub struct RunTally {
    runs: BTreeMap<String, RunUsage>,
    lanes: BTreeMap<(String, usize), Tally>,
    spawned: HashMap<(String, usize), (String, String)>,
}

impl RunTally {
    pub fn spawned(&mut self, session: &str, index: usize, kind: &str, summary: &str) {
        self.spawned.insert((session.to_owned(), index), (kind.to_owned(), summary.to_owned()));
    }

    pub fn used(&mut self, session: &str, agent: Option<&String>, lane: Option<usize>, usage: &ModelUsage) {
        let run = self.runs.entry(session.to_owned()).or_insert_with(|| RunUsage {
            session: session.to_owned(),
            agent: agent.cloned(),
            ..RunUsage::default()
        });
        if run.item.is_none() {
            run.item.clone_from(&usage.attribution.item);
            run.step.clone_from(&usage.attribution.step);
        }
        run.tally.add(usage);
        match lane {
            Some(index) => self.lanes.entry((session.to_owned(), index)).or_default().add(usage),
            None => run.lead.add(usage),
        }
    }

    pub fn report(mut self) -> Vec<RunUsage> {
        for ((session, index), tally) in self.lanes {
            let (kind, summary) = self.spawned.remove(&(session.clone(), index)).unwrap_or_default();
            if let Some(run) = self.runs.get_mut(&session) {
                run.subagents.push(SubagentUsage { index, kind, summary, tally });
            }
        }
        let mut runs: Vec<RunUsage> = self.runs.into_values().collect();
        for run in &mut runs {
            run.subagents.sort_by(|a, b| ranking(&a.tally, &b.tally).then(a.index.cmp(&b.index)));
        }
        runs.sort_by(|a, b| ranking(&a.tally, &b.tally).then(a.session.cmp(&b.session)));
        runs.truncate(SHOWN);
        runs
    }
}

#[cfg(test)]
#[path = "usage_runs_tests.rs"]
mod tests;
