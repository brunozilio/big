use serde::{Deserialize, Serialize};

use crate::billing::{Cost, micros};
use crate::quota::{QuotaError, Scope};

const SECTION: &str = "budget";

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Unpriced {
    #[default]
    Block,
    Estimate {
        micros_per_call: u64,
    },
    Allow,
}

impl Unpriced {
    pub fn configured() -> Self {
        Self::read(brand::section(SECTION))
    }

    pub fn read(pairs: &[(&str, &str)]) -> Self {
        let value = |key: &str| pairs.iter().find(|(name, _)| *name == key).map(|(_, value)| value.trim());
        match value("unpriced") {
            Some("allow") => Self::Allow,
            Some("estimate") => Self::Estimate {
                micros_per_call: value("unpriced_usd").and_then(|usd| usd.parse::<f64>().ok()).map_or(0, micros),
            },
            _ => Self::Block,
        }
    }

    fn counted(self, cost: Cost) -> Option<u64> {
        match self {
            Self::Block if cost.unpriced_calls > 0 => None,
            Self::Block | Self::Allow => Some(cost.known_micros),
            Self::Estimate { micros_per_call } => {
                Some(cost.known_micros.saturating_add(cost.unpriced_calls.saturating_mul(micros_per_call)))
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Budget {
    pub limit_usd: f64,
    pub spent: Cost,
}

impl Budget {
    pub fn check(&self, run: Cost, unpriced: Unpriced) -> Result<(), QuotaError> {
        let total = self.spent.plus(run);
        let limit_micros = micros(self.limit_usd);
        let counted = unpriced.counted(total);
        match counted {
            Some(counted) if counted < limit_micros => Ok(()),
            _ => Err(QuotaError::Budget {
                scope: Scope::Session,
                limit_micros,
                counted_micros: counted.unwrap_or(total.known_micros),
                spent: total,
                resets_at: None,
            }),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Spending {
    pub limit_usd: Option<f64>,
    pub spent: Cost,
}

impl Spending {
    pub fn observe(&mut self, event: &crate::EngineEvent) {
        self.spent.add(spent(event));
    }

    pub fn budget(&self) -> Option<Budget> {
        self.limit_usd.map(|limit_usd| Budget { limit_usd, spent: self.spent })
    }

    pub fn check(&self, run: Cost, unpriced: Unpriced) -> Result<(), QuotaError> {
        self.budget().map_or(Ok(()), |budget| budget.check(run, unpriced))
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BudgetCommand {
    Show,
    Off,
    Set(f64),
}

pub fn parse(argument: Option<&str>) -> Result<BudgetCommand, String> {
    let text = argument.unwrap_or_default().trim();
    match text {
        "" => Ok(BudgetCommand::Show),
        "off" | "none" => Ok(BudgetCommand::Off),
        _ => match text.trim_start_matches('$').parse::<f64>() {
            Ok(limit) if limit > 0.0 && limit.is_finite() => Ok(BudgetCommand::Set(limit)),
            _ => Err("Use /budget <dollars> (for example /budget 5), or /budget off.".into()),
        },
    }
}

pub fn spent(event: &crate::EngineEvent) -> Cost {
    match event {
        crate::EngineEvent::Usage(usage) => Cost::of(usage),
        crate::EngineEvent::AgentEvent { event, .. } => spent(event),
        _ => Cost::default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn known(usd: f64) -> Cost {
        Cost::priced(Some(usd), crate::billing::Basis::Api)
    }

    fn unpriced() -> Cost {
        Cost::priced(None, crate::billing::Basis::Api)
    }

    #[test]
    fn a_budget_allows_calls_until_the_session_has_spent_it() {
        let budget = Budget { limit_usd: 5.0, spent: known(4.0) };
        assert!(budget.check(known(0.5), Unpriced::Block).is_ok());
        let refused = budget.check(known(1.25), Unpriced::Block).expect_err("spent");
        assert_eq!(
            refused.to_string(),
            "Stopped: this session spent $5.25 of its $5.00 budget. Raise it with /budget <dollars>, or turn it off with /budget off."
        );
    }

    #[test]
    fn a_call_without_a_price_counts_against_the_budget_as_the_config_says() {
        let budget = Budget { limit_usd: 1.0, spent: known(0.5).plus(unpriced()) };
        let blocked = budget.check(Cost::default(), Unpriced::Block).expect_err("blocked");
        assert!(
            matches!(blocked, QuotaError::Budget { scope: Scope::Session, spent, .. } if spent.unpriced_calls == 1)
        );
        assert!(budget.check(Cost::default(), Unpriced::Allow).is_ok());
        assert!(budget.check(Cost::default(), Unpriced::Estimate { micros_per_call: 400_000 }).is_ok());
        let estimated = budget.check(unpriced(), Unpriced::Estimate { micros_per_call: 400_000 });
        assert!(matches!(estimated, Err(QuotaError::Budget { counted_micros: 1_300_000, .. })), "{estimated:?}");
    }

    #[test]
    fn the_config_names_the_unpriced_policy() {
        assert_eq!(Unpriced::read(&[]), Unpriced::Block);
        assert_eq!(Unpriced::read(&[("unpriced", "allow")]), Unpriced::Allow);
        assert_eq!(
            Unpriced::read(&[("unpriced", "estimate"), ("unpriced_usd", "0.05")]),
            Unpriced::Estimate { micros_per_call: 50_000 }
        );
        assert_eq!(Unpriced::read(&[("unpriced", "anything")]), Unpriced::Block);
        assert_ne!(
            Unpriced::configured(),
            Unpriced::Allow,
            "the shipped config never lets an unknown cost pass as zero"
        );
    }

    #[test]
    fn a_session_s_spending_adds_up_what_its_runs_cost_and_is_the_budget_of_the_next_one() {
        let usage = |cost| {
            crate::EngineEvent::Usage(crate::billing::ModelUsage {
                estimated_cost_usd: Some(cost),
                ..Default::default()
            })
        };
        let mut spending = Spending::default();
        spending.observe(&usage(1.5));
        spending.observe(&crate::EngineEvent::AgentEvent { index: 1, event: Box::new(usage(0.5)) });
        spending.observe(&crate::EngineEvent::TextDelta("hi".into()));
        assert_eq!(spending.spent.known_micros, 2_000_000);
        assert_eq!(spending.budget(), None, "no limit, no budget");
        assert!(spending.check(known(1_000.0), Unpriced::Block).is_ok(), "and nothing is refused");

        spending.limit_usd = Some(3.0);
        assert_eq!(spending.budget(), Some(Budget { limit_usd: 3.0, spent: spending.spent }));
        assert!(spending.check(known(0.5), Unpriced::Block).is_ok());
        assert!(spending.check(known(1.0), Unpriced::Block).is_err());
    }

    #[test]
    fn the_command_shows_sets_or_turns_off_the_budget() {
        assert_eq!(parse(None), Ok(BudgetCommand::Show));
        assert_eq!(parse(Some("off")), Ok(BudgetCommand::Off));
        assert_eq!(parse(Some("$2.50")), Ok(BudgetCommand::Set(2.5)));
        assert_eq!(parse(Some(" 10 ")), Ok(BudgetCommand::Set(10.0)));
        assert!(parse(Some("-1")).is_err());
        assert!(parse(Some("lots")).is_err());
    }

    #[test]
    fn spending_counts_sub_agents_and_calls_without_a_price_too() {
        let usage = |cost| {
            crate::EngineEvent::Usage(crate::billing::ModelUsage { estimated_cost_usd: cost, ..Default::default() })
        };
        assert_eq!(spent(&usage(Some(0.25))).known_micros, 250_000);
        let nested = crate::EngineEvent::AgentEvent { index: 1, event: Box::new(usage(Some(0.5))) };
        assert_eq!(spent(&nested).known_micros, 500_000);
        assert_eq!(spent(&usage(None)).unpriced_calls, 1, "an unknown price is not a zero");
        assert_eq!(spent(&crate::EngineEvent::Cancelled), Cost::default());
    }
}
