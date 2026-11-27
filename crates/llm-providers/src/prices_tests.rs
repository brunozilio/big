use super::*;

fn declared(json: serde_json::Value) -> Prices {
    serde_json::from_value(json).unwrap_or_else(|error| panic!("{error}"))
}

#[test]
fn a_line_is_usd_per_million_and_a_later_tier_takes_over_above_its_size() {
    let line = "0.1 0.5 0.01 0.125 | 100000 0.5 2.5 0.05 0.625";
    let input = |tokens| tier(line, tokens).map(|rates| rates.input);
    assert_eq!(input(100_000), Some(100_000));
    assert_eq!(input(100_001), Some(500_000));
    assert_eq!(
        tier("4 20 0.4 5", 0),
        Some(TokenRates { input: 4_000_000, output: 20_000_000, cache_read: 400_000, cache_write: 5_000_000 })
    );
    assert_eq!(tier("4 20", 0), None);
    assert_eq!(tier("x 1 1 1", 0), None);
}

#[test]
fn model_and_provider_names_match_their_config_keys() {
    assert_eq!(key("copilot:claude-sonnet-4.5"), "claude_sonnet_4_5");
    assert_eq!(key("kiro-subscription"), "kiro_subscription");
}

#[test]
fn a_declared_price_is_found_by_any_spelling_of_the_model_and_tiers_by_prompt_size() {
    let prices = declared(serde_json::json!({
        "claude-haiku-5.5": { "input": 0.1, "output": 0.5, "cacheRead": 0.01, "cacheWrite": 0.125,
            "tiers": [{ "above": 100_000, "input": 0.5, "output": 2.5, "cacheRead": 0.05, "cacheWrite": 0.625 }] },
    }));
    let input = |model: &str, tokens| prices.rates(model, tokens, Cache::Short).map(|rates| rates.input);
    assert_eq!(input("claude-haiku-5-5", 100_000), Some(100_000));
    assert_eq!(input("kiro:global.anthropic.claude-haiku-5.5", 100_001), Some(500_000));
    assert_eq!(input("claude-haiku-4-5", 0), None);
    assert!(NONE.rates("claude-haiku-5-5", 0, Cache::Short).is_none());
}

#[test]
fn the_cache_write_can_be_priced_per_lifetime() {
    let prices = declared(serde_json::json!({
        "m": { "input": 1, "output": 2, "cacheRead": 0.1, "cacheWrite": { "short": 1.25, "long": 2 } },
        "flat": { "input": 1, "output": 2, "cacheRead": 0.1, "cacheWrite": 1.25, "laterDiscount": 0.25 },
    }));
    let write = |model: &str, cache| prices.rates(model, 0, cache).map(|rates| rates.cache_write);
    assert_eq!(write("m", Cache::Long), Some(2_000_000));
    assert_eq!(write("m", Cache::Short), Some(1_250_000));
    assert_eq!(write("m", Cache::None), Some(1_250_000), "an unlisted lifetime takes the short one");
    assert_eq!(write("flat", Cache::Long), Some(1_250_000));
    assert_eq!(prices.price("flat").and_then(|price| price.later_discount), Some(0.25));
}

#[test]
fn without_an_override_in_the_config_nothing_is_overridden() {
    assert!(overridden("claude-subscription", "claude-opus-5-5", 0).is_none());
}
