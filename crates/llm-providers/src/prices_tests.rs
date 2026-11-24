use super::*;

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
fn each_subscription_is_priced_by_its_configured_source() {
    assert_eq!(source("kiro-subscription"), Some("bedrock openai"));
    assert_eq!(source("copilot-subscription"), Some("openai anthropic vertex"));
    assert_eq!(source("openai-subscription"), Some("openai"));
    assert_eq!(source("claude-subscription"), Some("anthropic"));
    assert!(listed("anthropic", "claude-opus-5-5", 0).is_some());
    assert!(listed("bedrock", "claude-sonnet-4.5", 0).is_some());
    assert!(listed("bedrock", "global.anthropic.claude-opus-5-5", 0).is_some());
    assert!(listed("anthropic", "unreviewed-model", 0).is_none());
}

#[test]
fn every_configured_source_has_a_price_table() {
    for (_, sources) in brand::section(SOURCES) {
        for source in sources.split_whitespace() {
            assert!(!brand::section(&format!("{TABLE}{source}")).is_empty(), "no [{TABLE}{source}] in the config");
        }
    }
    let input = |model: &str, tokens| listed("xai", model, tokens).map(|rates| rates.input);
    assert_eq!(input("grok-subscription:grok-4.6", 199_999), Some(2_000_000));
    assert_eq!(input("grok-subscription:grok-4.6", 200_000), Some(4_000_000), "from 200k the whole request costs more");
    assert_eq!(input("grok-subscription:grok-4.20-0309-reasoning", 0), Some(1_250_000));
}
