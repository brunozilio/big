use serde::{Deserialize, Serialize};

const MICROS: f64 = 1_000_000.0;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Basis {
    #[default]
    Unknown,
    Api,
    Subscription,
    Mixed,
}

impl Basis {
    pub fn of(recorded: &str) -> Self {
        let recorded = recorded.trim().to_ascii_lowercase();
        if recorded.starts_with("subscription") {
            Self::Subscription
        } else if recorded.starts_with("api") {
            Self::Api
        } else if recorded == "mixed" {
            Self::Mixed
        } else {
            Self::Unknown
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Unknown => "",
            Self::Api => "api",
            Self::Subscription => "subscription",
            Self::Mixed => "mixed",
        }
    }

    pub fn joined(self, other: Self) -> Self {
        match (self, other) {
            (Self::Unknown, other) => other,
            (kept, Self::Unknown) => kept,
            (kept, other) if kept == other => kept,
            _ => Self::Mixed,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Cost {
    pub known_micros: u64,
    pub unpriced_calls: u64,
    pub basis: Basis,
}

impl Cost {
    pub fn priced(usd: Option<f64>, basis: Basis) -> Self {
        match usd {
            Some(usd) => Self { known_micros: micros(usd), unpriced_calls: 0, basis },
            None => Self { known_micros: 0, unpriced_calls: 1, basis },
        }
    }

    pub fn of(usage: &ModelUsage) -> Self {
        Self::priced(usage.estimated_cost_usd, Basis::of(&usage.basis))
    }

    pub fn add(&mut self, other: Self) {
        self.known_micros = self.known_micros.saturating_add(other.known_micros);
        self.unpriced_calls = self.unpriced_calls.saturating_add(other.unpriced_calls);
        self.basis = self.basis.joined(other.basis);
    }

    pub fn plus(mut self, other: Self) -> Self {
        self.add(other);
        self
    }

    pub fn known_usd(&self) -> f64 {
        self.known_micros as f64 / MICROS
    }

    pub const fn is_partial(&self) -> bool {
        self.unpriced_calls > 0
    }
}

pub fn micros(usd: f64) -> u64 {
    if usd.is_finite() && usd > 0.0 { (usd * MICROS).round() as u64 } else { 0 }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TokenRates {
    pub input: u64,
    pub output: u64,
    pub cache_read: u64,
    pub cache_write: u64,
}
impl TokenRates {
    pub fn estimate(self, input: u64, output: u64, cache_read: u64, cache_write: u64) -> f64 {
        let micro = u128::from(input) * u128::from(self.input)
            + u128::from(output) * u128::from(self.output)
            + u128::from(cache_read) * u128::from(self.cache_read)
            + u128::from(cache_write) * u128::from(self.cache_write);
        micro as f64 / 1_000_000_000_000.0
    }
}

pub fn searches_cost(micro_per_search: u64, searches: u64) -> f64 {
    (u128::from(micro_per_search) * u128::from(searches)) as f64 / 1_000_000.0
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ModelUsage {
    pub model: String,
    pub input: u64,
    pub output: u64,
    pub cache_read: u64,
    pub cache_write: u64,
    #[serde(default)]
    pub searches: u64,
    pub estimated_cost_usd: Option<f64>,
    pub basis: String,
    pub price_date: String,
    #[serde(default)]
    pub provider: String,
    #[serde(default)]
    pub account: String,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct SessionUsage {
    pub models: Vec<ModelUsage>,
}

impl SessionUsage {
    pub fn add(&mut self, call: &ModelUsage) {
        let Some(kept) = self.models.iter_mut().find(|kept| kept.model == call.model) else {
            return self.models.push(call.clone());
        };
        kept.input += call.input;
        kept.output += call.output;
        kept.cache_read += call.cache_read;
        kept.cache_write += call.cache_write;
        kept.searches += call.searches;
        kept.estimated_cost_usd = match (kept.estimated_cost_usd, call.estimated_cost_usd) {
            (None, None) => None,
            (kept, call) => Some(kept.unwrap_or(0.0) + call.unwrap_or(0.0)),
        };
    }
}

pub fn cache_hit_rate(usage: &ModelUsage) -> Option<f64> {
    let prompt = usage.input + usage.cache_read + usage.cache_write;
    (prompt > 0).then(|| usage.cache_read as f64 / prompt as f64)
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PlanUsage {
    pub plan: Option<String>,
    pub windows: Vec<PlanWindow>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PlanWindow {
    pub id: String,
    pub label: String,
    pub used_percent: f64,
    pub resets_at: Option<String>,
    #[serde(default)]
    pub starts_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PlanLimits {
    Known(PlanUsage),
    Unsupported,
    Failed(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct AccountLimits {
    pub provider: crate::ModelProvider,
    pub account: String,
    pub limits: PlanLimits,
}

#[cfg(test)]
mod tests {
    use super::*;
    fn call(model: &str, input: u64, cache_read: u64, cost: Option<f64>) -> ModelUsage {
        ModelUsage {
            model: model.into(),
            input,
            output: 10,
            cache_read,
            cache_write: 0,
            searches: 1,
            estimated_cost_usd: cost,
            basis: String::new(),
            price_date: String::new(),
            provider: String::new(),
            account: String::new(),
        }
    }

    #[test]
    fn a_session_adds_up_its_calls_per_model() {
        let mut session = SessionUsage::default();
        session.add(&call("opus", 100, 900, Some(0.5)));
        session.add(&call("haiku", 50, 0, None));
        session.add(&call("opus", 100, 900, Some(0.25)));
        assert_eq!(session.models.len(), 2);
        let opus = &session.models[0];
        assert_eq!((opus.model.as_str(), opus.input, opus.output, opus.cache_read), ("opus", 200, 20, 1800));
        assert_eq!(opus.estimated_cost_usd, Some(0.75));
        assert_eq!(session.models[1].estimated_cost_usd, None, "an unpriced model stays unpriced");
    }

    #[test]
    fn a_search_costs_its_price_on_top_of_tokens() {
        assert_eq!(searches_cost(10_000, 3), 0.03);
        let mut session = SessionUsage::default();
        session.add(&call("opus", 1, 0, Some(0.01)));
        session.add(&call("opus", 1, 0, Some(0.01)));
        assert_eq!(session.models[0].searches, 2);
    }

    #[test]
    fn a_cost_keeps_what_is_known_apart_from_the_calls_without_a_price() {
        let priced = |cost: f64| ModelUsage { basis: "subscription".into(), ..call("opus", 1, 0, Some(cost)) };
        let mut cost = Cost::default();
        cost.add(Cost::of(&priced(0.5)));
        cost.add(Cost::of(&priced(0.25)));
        cost.add(Cost::of(&call("local", 1, 0, None)));
        assert_eq!((cost.known_micros, cost.unpriced_calls), (750_000, 1));
        assert!(cost.is_partial());
        assert!((cost.known_usd() - 0.75).abs() < f64::EPSILON);
        assert_eq!(cost.basis, Basis::Subscription);
        assert_eq!(cost.plus(Cost::priced(Some(0.1), Basis::Api)).basis, Basis::Mixed);
    }

    #[test]
    fn the_basis_reads_both_the_declared_word_and_the_older_recorded_label() {
        assert_eq!(Basis::of("subscription-api-equivalent"), Basis::Subscription);
        assert_eq!(Basis::of("api-reference-estimate"), Basis::Api);
        assert_eq!(Basis::of(Basis::Api.label()), Basis::Api);
        assert_eq!(Basis::of(""), Basis::Unknown);
    }

    #[test]
    fn the_cache_hit_rate_is_the_share_of_prompt_tokens_read_from_cache() {
        assert_eq!(cache_hit_rate(&call("opus", 100, 900, None)), Some(0.9));
        assert_eq!(cache_hit_rate(&call("opus", 0, 0, None)), None);
    }
}
