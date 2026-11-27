use domain::billing::TokenRates;

use crate::Vendor as Family;

#[derive(Debug, Clone, Copy, Default)]
pub struct Vendor;

const SEARCH: u64 = 10_000;

impl Family for Vendor {
    fn id(&self) -> &'static str {
        "anthropic"
    }

    fn owns(&self, model: &str) -> bool {
        model.starts_with("claude-")
    }

    fn search_price(&self) -> Option<u64> {
        Some(SEARCH)
    }

    fn rates(&self, model: &str, prompt_tokens: u64) -> Option<TokenRates> {
        crate::prices::listed(self.id(), model, prompt_tokens)
    }

    fn alias(&self, name: &str) -> Option<&'static str> {
        match name {
            "sonnet" => Some("claude-sonnet-5-5"),
            "opus" => Some("claude-opus-5-5"),
            "haiku" => Some("claude-haiku-5-5"),
            _ => None,
        }
    }

    fn small(&self) -> Option<&'static str> {
        Some("claude-haiku-5-5")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prices_are_known_per_model_and_by_prompt_size() {
        let rate = Vendor.rates("claude-sonnet-5-5", 100).expect("priced");
        assert!((rate.estimate(100, 10, 20, 5) - 0.0003145).abs() < 1e-12);
        let input = |tokens| Vendor.rates("claude-haiku-5-5", tokens).map(|rate| rate.input);
        assert_eq!((input(100), input(100_001)), (Some(100_000), Some(500_000)));
        assert!(Vendor.rates("unreviewed-model", 1).is_none());
    }

    #[test]
    fn short_names_pick_the_current_models() {
        assert_eq!(Vendor.alias("opus"), Some("claude-opus-5-5"));
        assert_eq!(Vendor.alias("gpt"), None);
    }
}
