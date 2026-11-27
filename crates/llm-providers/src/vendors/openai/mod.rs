use domain::billing::TokenRates;

use crate::Vendor as Family;

#[derive(Debug, Clone, Copy, Default)]
pub struct Vendor;

const SEARCH: u64 = 10_000;

impl Family for Vendor {
    fn id(&self) -> &'static str {
        "openai"
    }

    fn owns(&self, model: &str) -> bool {
        model.starts_with("gpt-") || ["o1", "o3", "o4"].iter().any(|prefix| model.starts_with(prefix))
    }

    fn small(&self) -> Option<&'static str> {
        Some("gpt-6-luna")
    }

    fn search_price(&self) -> Option<u64> {
        Some(SEARCH)
    }

    fn rates(&self, model: &str, prompt_tokens: u64) -> Option<TokenRates> {
        crate::prices::listed(self.id(), model, prompt_tokens)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn its_models_are_priced_and_owned() {
        assert!(Vendor.rates("gpt-6.1-sol", 1).is_some());
        assert!(Vendor.owns("o3-mini") && !Vendor.owns("claude-opus-5-5"));
    }
}
