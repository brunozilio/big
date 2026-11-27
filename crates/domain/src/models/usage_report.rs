use std::collections::{BTreeMap, BTreeSet, HashMap};

use crate::EngineEvent;
use crate::billing::ModelUsage;

pub const UNKNOWN: &str = "unknown";

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Tally {
    pub cost_usd: f64,
    pub tokens: u64,
    pub calls: u64,
}

impl Tally {
    fn add(&mut self, usage: &ModelUsage) {
        self.cost_usd += usage.estimated_cost_usd.unwrap_or(0.0);
        self.tokens += usage.input + usage.output + usage.cache_read + usage.cache_write;
        self.calls += 1;
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Totals {
    pub cost_usd: f64,
    pub input: u64,
    pub output: u64,
    pub cache_read: u64,
    pub cache_write: u64,
    pub calls: u64,
    pub sessions: u64,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct DayUsage {
    pub date: String,
    pub tally: Tally,
    pub by_provider: BTreeMap<String, f64>,
    pub by_model: BTreeMap<String, f64>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Ranked {
    pub id: String,
    pub tally: Tally,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct AccountRanked {
    pub provider: String,
    pub account: String,
    pub tally: Tally,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct UsageReport {
    pub totals: Totals,
    pub daily: Vec<DayUsage>,
    pub by_provider: Vec<Ranked>,
    pub by_model: Vec<Ranked>,
    pub by_account: Vec<AccountRanked>,
    pub by_agent: Vec<Ranked>,
}

#[derive(Debug, Clone, Default)]
pub struct UsageTally {
    known: Vec<String>,
    totals: Totals,
    daily: Vec<DayUsage>,
    sessions: BTreeSet<String>,
    providers: BTreeMap<String, Tally>,
    models: BTreeMap<String, Tally>,
    accounts: BTreeMap<(String, String), Tally>,
    agent_of: HashMap<String, String>,
    agents: BTreeMap<String, Tally>,
}

impl UsageTally {
    pub fn new(window: &[String], known: &[&str]) -> Self {
        Self {
            known: known.iter().map(|id| (*id).to_owned()).collect(),
            daily: window.iter().map(|date| DayUsage { date: date.clone(), ..DayUsage::default() }).collect(),
            ..Self::default()
        }
    }

    pub fn with_agents(self, agent_of: HashMap<String, String>) -> Self {
        Self { agent_of, ..self }
    }

    pub fn add(&mut self, session: &str, date: &str, event: &EngineEvent) {
        match event {
            EngineEvent::Usage(usage) => self.used(session, date, usage),
            EngineEvent::AgentEvent { event, .. } => self.add(session, date, event),
            _ => {}
        }
    }

    fn used(&mut self, session: &str, date: &str, usage: &ModelUsage) {
        let Some(day) = self.daily.iter_mut().find(|day| day.date == date) else {
            return;
        };
        let (provider, model) = served(usage, &self.known);
        let cost = usage.estimated_cost_usd.unwrap_or(0.0);
        day.tally.add(usage);
        *day.by_provider.entry(provider.clone()).or_default() += cost;
        *day.by_model.entry(model.clone()).or_default() += cost;
        self.totals.cost_usd += cost;
        self.totals.input += usage.input;
        self.totals.output += usage.output;
        self.totals.cache_read += usage.cache_read;
        self.totals.cache_write += usage.cache_write;
        self.totals.calls += 1;
        self.sessions.insert(session.to_owned());
        self.providers.entry(provider.clone()).or_default().add(usage);
        self.models.entry(model).or_default().add(usage);
        self.accounts.entry((provider, usage.account.clone())).or_default().add(usage);
        if let Some(agent) = self.agent_of.get(session) {
            self.agents.entry(agent.clone()).or_default().add(usage);
        }
    }

    pub fn report(self) -> UsageReport {
        let ranked = |tallies: BTreeMap<String, Tally>| {
            let mut ranked: Vec<Ranked> = tallies.into_iter().map(|(id, tally)| Ranked { id, tally }).collect();
            ranked
                .sort_by(|a, b| b.tally.cost_usd.total_cmp(&a.tally.cost_usd).then(b.tally.calls.cmp(&a.tally.calls)));
            ranked
        };
        let mut by_account: Vec<AccountRanked> = self
            .accounts
            .into_iter()
            .map(|((provider, account), tally)| AccountRanked { provider, account, tally })
            .collect();
        by_account
            .sort_by(|a, b| b.tally.cost_usd.total_cmp(&a.tally.cost_usd).then(b.tally.calls.cmp(&a.tally.calls)));
        UsageReport {
            totals: Totals { sessions: self.sessions.len() as u64, ..self.totals },
            daily: self.daily,
            by_provider: ranked(self.providers),
            by_model: ranked(self.models),
            by_account,
            by_agent: ranked(self.agents),
        }
    }
}

fn served(usage: &ModelUsage, known: &[String]) -> (String, String) {
    let known: Vec<&str> = known.iter().map(String::as_str).collect();
    let route = crate::routing::route(&usage.model, &known);
    let provider = match (usage.provider.trim(), route.provider) {
        ("", Some(prefix)) => prefix,
        ("", None) => UNKNOWN,
        (recorded, _) => recorded,
    };
    (provider.to_owned(), route.model.to_owned())
}

#[cfg(test)]
#[path = "usage_report_tests.rs"]
mod tests;
