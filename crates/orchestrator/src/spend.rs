use std::sync::{Mutex, PoisonError};

use domain::EngineEvent;
use domain::billing::Cost;
use domain::budget::Budget;
use domain::ceilings::Ceilings;
use domain::origin::{Attribution, Origin};
use domain::quota::Scope;
use domain::workflow::ItemId;
use jiff::{ToSpan, Zoned};
use store::{Spent, Store};

use crate::protocol::Command;

pub fn attribution(item: &ItemId, step: &str, command: &Command) -> Attribution {
    let origin = command.args["origin"].as_str().and_then(Origin::named).unwrap_or_default();
    Attribution::of_step(item.to_string(), step, origin)
}

fn origin_of(event: &EngineEvent) -> Origin {
    match event {
        EngineEvent::Usage(usage) => usage.attribution.origin,
        EngineEvent::AgentEvent { event, .. } => origin_of(event),
        _ => Origin::Step,
    }
}

impl crate::agent::AgentEffect<'_> {
    pub(crate) fn charge_to(&self, name: &str, state: &domain::workflow::WorkItem, budget_usd: Option<f64>) {
        let application = crate::application::id_of(&state.input);
        let owner = crate::owners::of(&self.shared.store, state);
        let charge = Charge::new(self.item, &self.key.step, name, application, &owner);
        let progress = state.effects.get(&self.key.to_string()).map(|entry| &entry.progress);
        let granted = progress.map(crate::overrun::granted_of).unwrap_or_default();
        let _ = self.charge.set(charge.with_agent_ceiling(budget_usd).with_granted(granted));
    }

    pub(crate) fn budgeted(&self, request: &domain::PromptRequest) -> domain::PromptRequest {
        let mut request = request.clone();
        let Some(charge) = self.charge.get() else { return request };
        let month = Month::of(&Zoned::now());
        request.policy.budgets = budgets(&self.shared.store, &Ceilings::configured(), charge, self.command, month);
        request
    }
}

#[derive(Debug, Default)]
pub struct Charge {
    pub item: String,
    pub step: String,
    pub agent: String,
    pub application: String,
    pub owner: String,
    pub agent_usd: Option<f64>,
    run: Mutex<Cost>,
    extended: Mutex<Vec<(Scope, f64)>>,
}

impl Charge {
    pub fn new(item: &ItemId, step: &str, agent: &str, application: Option<String>, owner: &str) -> Self {
        Self {
            item: item.to_string(),
            step: step.to_owned(),
            agent: agent.to_owned(),
            application: application.unwrap_or_default(),
            owner: owner.to_owned(),
            ..Self::default()
        }
    }

    pub fn with_agent_ceiling(self, agent_usd: Option<f64>) -> Self {
        Self { agent_usd, ..self }
    }

    pub fn run_spent(&self) -> Cost {
        *self.run.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub fn extend(&self, scope: Scope, reached_usd: f64) {
        let once = (reached_usd - self.extra(scope)).max(0.0);
        self.extended.lock().unwrap_or_else(PoisonError::into_inner).push((scope, once));
    }

    pub fn with_granted(self, granted: Vec<(Scope, f64)>) -> Self {
        Self { extended: Mutex::new(granted), ..self }
    }

    pub fn granted(&self) -> Vec<(Scope, f64)> {
        self.extended.lock().unwrap_or_else(PoisonError::into_inner).clone()
    }

    pub fn extensions(&self) -> usize {
        self.extended.lock().unwrap_or_else(PoisonError::into_inner).len()
    }

    fn extra(&self, scope: Scope) -> f64 {
        let extended = self.extended.lock().unwrap_or_else(PoisonError::into_inner);
        extended.iter().filter(|(of, _)| *of == scope).map(|(_, usd)| usd).sum()
    }

    pub fn record(&self, store: &Store, event: &EngineEvent, at: i64) {
        let cost = domain::budget::spent(event);
        if cost == Cost::default() {
            return;
        }
        self.run.lock().unwrap_or_else(PoisonError::into_inner).add(cost);
        let spent = Spent {
            at,
            item: self.item.clone(),
            step: self.step.clone(),
            agent: self.agent.clone(),
            application: self.application.clone(),
            owner: self.owner.clone(),
            origin: origin_of(event),
            cost,
        };
        if let Err(error) = store.record_spend(&spent) {
            tracing::warn!(%error, item = %self.item, "a run's spend could not be kept");
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Month {
    pub since_ms: i64,
    pub resets_at: i64,
}

impl Month {
    pub fn of(now: &Zoned) -> Option<Self> {
        let first = now.date().first_of_month();
        let start = first.to_zoned(now.time_zone().clone()).ok()?;
        let next = first.checked_add(1.month()).ok()?.to_zoned(now.time_zone().clone()).ok()?;
        Some(Self { since_ms: start.timestamp().as_millisecond(), resets_at: next.timestamp().as_second() })
    }
}

pub fn budgets(
    store: &Store,
    ceilings: &Ceilings,
    charge: &Charge,
    command: &Command,
    month: Option<Month>,
) -> Vec<Budget> {
    let mut found = Vec::new();
    if let Some(limit_usd) = ceilings.run_of(&command.args["budgetUsd"], charge.agent_usd) {
        let limit_usd = limit_usd + charge.extra(Scope::Run);
        found.push(Budget { limit_usd, spent: charge.run_spent(), scope: Scope::Run, resets_at: None });
    }
    let Some(month) = month else { return found };
    for (scope, id) in
        [(Scope::Agent, &charge.agent), (Scope::Application, &charge.application), (Scope::Person, &charge.owner)]
    {
        let Some(limit_usd) = ceilings.of(scope).filter(|_| !id.is_empty()) else { continue };
        let spent = store.spent_since(month.since_ms, scope, id).unwrap_or_else(|error| {
            tracing::warn!(%error, scope = scope.word(), "the spend so far could not be read; the ceiling counts from zero");
            Cost::default()
        });
        found.push(Budget {
            limit_usd: limit_usd + charge.extra(scope),
            spent,
            scope,
            resets_at: Some(month.resets_at),
        });
    }
    found
}

#[cfg(test)]
#[path = "spend_tests.rs"]
mod tests;
