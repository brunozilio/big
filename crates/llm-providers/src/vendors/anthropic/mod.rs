use domain::ModelOption;
use domain::billing::TokenRates;
use serde_json::{Value, json};

use crate::request::{capabilities, remove, unsupported};
use crate::{ProviderError, Vendor as Family};

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

    fn efforts(&self, model: &str) -> Vec<&'static str> {
        let dialed = modern(model) || adaptive_4(model) || model.starts_with("claude-opus-4-5");
        if dialed { crate::EFFORTS.to_vec() } else { Vec::new() }
    }

    fn effort(
        &self,
        body: &mut Value,
        selected: Option<&ModelOption>,
        effort: &str,
        _always: bool,
    ) -> Result<(), ProviderError> {
        let caps = capabilities(body, selected);
        let model = body["model"].as_str().unwrap_or_default().to_owned();
        let adaptive = modern(&model) || caps.is_some_and(|c| c.adaptive_thinking) || adaptive_4(&model);
        let supports = adaptive || model.starts_with("claude-opus-4-5");
        if supports {
            if unsupported(caps, effort) {
                return Err(ProviderError::Invalid(format!("model {model} does not support effort {effort}")));
            }
            body["output_config"] = json!({"effort":effort});
            if adaptive {
                body["thinking"] = json!({"type":"adaptive"});
            }
        } else if body["thinking"]["type"] == "enabled" {
            let max = body["max_tokens"].as_u64().unwrap_or(8192);
            let budget = match effort {
                "low" => max / 8,
                "medium" => max / 4,
                "high" => max / 2,
                "xhigh" => max * 3 / 4,
                _ => max * 7 / 8,
            }
            .max(1024);
            if max > 1024 {
                body["thinking"]["budget_tokens"] = json!(budget.min(max - 1));
            } else {
                remove(body, &["thinking"]);
            }
        }
        Ok(())
    }
}

fn modern(model: &str) -> bool {
    ["opus", "sonnet", "haiku"].iter().any(|family| {
        model
            .strip_prefix("claude-")
            .and_then(|rest| rest.strip_prefix(family))
            .and_then(|rest| rest.strip_prefix('-'))
            .is_some_and(|rest| version_digit(rest, '5'..='9'))
    })
}

fn adaptive_4(model: &str) -> bool {
    ["opus", "sonnet"].iter().any(|family| {
        model
            .strip_prefix("claude-")
            .and_then(|rest| rest.strip_prefix(family))
            .and_then(|rest| rest.strip_prefix("-4-"))
            .is_some_and(|rest| version_digit(rest, '6'..='9'))
    })
}

fn version_digit(rest: &str, range: std::ops::RangeInclusive<char>) -> bool {
    let mut chars = rest.chars();
    chars.next().is_some_and(|c| range.contains(&c)) && matches!(chars.next(), None | Some('-'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modern_claude_models_think_adaptively_with_the_effort() {
        for model in ["claude-opus-5", "claude-sonnet-5-5", "claude-opus-4-6"] {
            let mut body = json!({"model": model});
            Vendor.effort(&mut body, None, "low", false).unwrap_or_else(|e| panic!("{e}"));
            assert_eq!(body["thinking"], json!({"type": "adaptive"}), "{model}");
            assert_eq!(body["output_config"], json!({"effort": "low"}));
        }
        assert!(!modern("claude-opus-50") && !adaptive_4("claude-opus-4-5"));
    }

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
