use serde::{Deserialize, Serialize};

use crate::billing::Cost;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Scope {
    Run,
    #[default]
    Session,
    Item,
    Account,
    Workspace,
    Agent,
    Application,
    Person,
}

impl Scope {
    pub const ALL: [Self; 8] = [
        Self::Run,
        Self::Session,
        Self::Item,
        Self::Account,
        Self::Workspace,
        Self::Agent,
        Self::Application,
        Self::Person,
    ];

    pub const fn word(self) -> &'static str {
        match self {
            Self::Run => "run",
            Self::Session => "session",
            Self::Item => "item",
            Self::Account => "account",
            Self::Workspace => "workspace",
            Self::Agent => "agent",
            Self::Application => "application",
            Self::Person => "person",
        }
    }

    pub fn named(word: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|scope| scope.word() == word)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum QuotaError {
    Budget { scope: Scope, limit_micros: u64, counted_micros: u64, spent: Cost, resets_at: Option<i64> },
    Window { scope: Scope, window: String, resets_at: Option<i64> },
    Model { scope: Scope, model: String, resets_at: Option<i64> },
    Integration { scope: Scope, integration: String, resets_at: Option<i64> },
    Session { scope: Scope, resets_at: Option<i64> },
}

impl QuotaError {
    pub const fn scope(&self) -> Scope {
        match self {
            Self::Budget { scope, .. }
            | Self::Window { scope, .. }
            | Self::Model { scope, .. }
            | Self::Integration { scope, .. }
            | Self::Session { scope, .. } => *scope,
        }
    }

    pub const fn resets_at(&self) -> Option<i64> {
        match self {
            Self::Budget { resets_at, .. }
            | Self::Window { resets_at, .. }
            | Self::Model { resets_at, .. }
            | Self::Integration { resets_at, .. }
            | Self::Session { resets_at, .. } => *resets_at,
        }
    }
}

fn dollars(micros: u64) -> f64 {
    micros as f64 / 1_000_000.0
}

fn budget(f: &mut std::fmt::Formatter<'_>, scope: Scope, limit: u64, counted: u64, spent: &Cost) -> std::fmt::Result {
    let (raise, raise_or_off, tail) = match scope {
        Scope::Session => (
            "Raise it with /budget <dollars>",
            "Raise it with /budget <dollars>, turn it off with /budget off,",
            ", or turn it off with /budget off",
        ),
        _ => ("Raise its ceiling in the config", "Raise its ceiling in the config", ""),
    };
    let scope = scope.word();
    let unpriced = spent.unpriced_calls;
    if counted < limit && unpriced > 0 {
        return write!(
            f,
            "Stopped: {unpriced} model call(s) in this {scope} have no price, so its ${:.2} budget cannot be checked (${:.2} known so far). {raise_or_off} or set how unpriced calls count in the config.",
            dollars(limit),
            spent.known_usd()
        );
    }
    let partial = if unpriced > 0 { format!(" (+{unpriced} unpriced)") } else { String::new() };
    write!(
        f,
        "Stopped: this {scope} spent ${:.2}{partial} of its ${:.2} budget. {raise}{tail}.",
        dollars(counted),
        dollars(limit)
    )
}

impl std::fmt::Display for QuotaError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Budget { scope, limit_micros, counted_micros, spent, .. } => {
                budget(f, *scope, *limit_micros, *counted_micros, spent)
            }
            Self::Window { scope, window, .. } => {
                write!(f, "Stopped: the {window} usage window of this {} is used up.", scope.word())
            }
            Self::Model { scope, model, .. } => {
                write!(f, "Stopped: this {} reached its limit for the model {model}.", scope.word())
            }
            Self::Integration { scope, integration, .. } => {
                write!(f, "Stopped: this {} reached its limit for {integration}.", scope.word())
            }
            Self::Session { scope, .. } => write!(f, "Stopped: this {} reached its usage limit.", scope.word()),
        }
    }
}

impl std::error::Error for QuotaError {}

impl From<&QuotaError> for crate::failure::FailureKind {
    fn from(error: &QuotaError) -> Self {
        match error {
            QuotaError::Budget { scope, limit_micros, counted_micros, resets_at, .. } => Self::Budget {
                scope: *scope,
                limit_micros: *limit_micros,
                counted_micros: *counted_micros,
                resets_at: *resets_at,
            },
            _ => Self::UsageLimit { resets_at: error.resets_at() },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_quota_error_names_its_scope_and_reset_and_travels_as_tagged_json() {
        let window = QuotaError::Window { scope: Scope::Account, window: "five_hour".into(), resets_at: Some(42) };
        assert_eq!((window.scope(), window.resets_at()), (Scope::Account, Some(42)));
        let json = serde_json::to_value(&window).expect("json");
        assert_eq!(json["kind"], "window");
        assert_eq!(json["resetsAt"], 42);
        assert_eq!(serde_json::from_value::<QuotaError>(json).expect("back"), window);
        assert_eq!(
            crate::failure::FailureKind::from(&window),
            crate::failure::FailureKind::UsageLimit { resets_at: Some(42) }
        );
    }

    #[test]
    fn a_budget_refusal_tells_a_spent_budget_from_one_that_cannot_be_checked() {
        let spent = Cost { known_micros: 400_000, unpriced_calls: 1, ..Cost::default() };
        let refused = |counted| QuotaError::Budget {
            scope: Scope::Session,
            limit_micros: 1_000_000,
            counted_micros: counted,
            spent,
            resets_at: None,
        };
        assert!(refused(400_000).to_string().contains("1 model call(s) in this session have no price"));
        assert_eq!(
            refused(1_250_000).to_string(),
            "Stopped: this session spent $1.25 (+1 unpriced) of its $1.00 budget. Raise it with /budget <dollars>, or turn it off with /budget off."
        );
    }
}
