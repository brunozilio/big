use std::collections::BTreeMap;

use domain::billing::TokenRates;

use super::provider;
use crate::model::Cache;

const BEFORE: &str = include_str!("prices_before.toml");

fn sections() -> BTreeMap<String, Vec<(String, String)>> {
    let mut found: BTreeMap<String, Vec<(String, String)>> = BTreeMap::new();
    let mut current = String::new();
    for line in BEFORE.lines().map(str::trim).filter(|line| !line.is_empty()) {
        if let Some(name) = line.strip_prefix('[').and_then(|rest| rest.strip_suffix(']')) {
            current = name.to_owned();
            continue;
        }
        let (key, value) = line.split_once(" = ").unwrap_or_else(|| panic!("{line}"));
        found.entry(current.clone()).or_default().push((key.to_owned(), value.trim_matches('"').to_owned()));
    }
    found
}

fn tier(line: &str, prompt: u64) -> TokenRates {
    let mut chosen = None;
    for (at, part) in line.split('|').enumerate() {
        let words: Vec<f64> = part.split_whitespace().map(|word| word.parse().unwrap_or(f64::NAN)).collect();
        let (above, usd) = if at == 0 { (0, &words[..]) } else { (words[0] as u64, &words[1..]) };
        let micro = |usd: f64| (usd * 1_000_000.0).round() as u64;
        if at == 0 || prompt > above {
            chosen = Some(TokenRates {
                input: micro(usd[0]),
                output: micro(usd[1]),
                cache_read: micro(usd[2]),
                cache_write: micro(usd[3]),
            });
        }
    }
    chosen.unwrap_or_else(|| panic!("{line}"))
}

fn boundaries(line: &str) -> Vec<u64> {
    let mut sizes = vec![0, 1_000];
    for part in line.split('|').skip(1) {
        let above: u64 = part.split_whitespace().next().and_then(|word| word.parse().ok()).unwrap_or_default();
        sizes.extend([above, above + 1, above * 2]);
    }
    sizes
}

#[test]
fn every_provider_declares_the_same_prices_the_config_listed_before() {
    let tables = sections();
    let mut checked = 0;
    for (subscription, sources) in &tables["pricing"] {
        let id = subscription.replace('_', "-");
        let provider = provider(Box::leak(id.into_boxed_str()));
        let lines: Vec<&(String, String)> =
            sources.split_whitespace().flat_map(|source| &tables[&format!("prices_{source}")]).collect();
        for (model, _) in &lines {
            let line = &lines.iter().find(|(known, _)| known == model).unwrap_or_else(|| panic!("{model}")).1;
            for prompt in boundaries(line) {
                let declared = crate::priced(provider, model, prompt, Cache::Short);
                assert_eq!(declared, Some(tier(line, prompt)), "{} {model} at {prompt}", provider.id());
                checked += 1;
            }
        }
        assert!(crate::priced(provider, "unreviewed-model", 0, Cache::Short).is_none());
    }
    assert!(checked > 100, "{checked}");
}

#[test]
fn the_vendors_that_charged_for_web_searches_still_do() {
    assert_eq!(provider("claude-subscription").search_price(), Some(10_000));
    assert_eq!(provider("openai-subscription").search_price(), Some(10_000));
    for id in ["copilot-subscription", "kiro-subscription", "grok-subscription", "antigravity-subscription"] {
        assert_eq!(provider(id).search_price(), None, "{id}");
    }
}

#[test]
fn a_model_s_declared_low_priority_discount_wins_over_the_provider_s() {
    let claude = provider("claude-subscription");
    assert_eq!(crate::later_discount(claude, "claude-sonnet-5-5"), Some(0.5));
    assert_eq!(crate::later_discount(provider("grok-subscription"), "grok-4.6"), None);
}

#[test]
fn a_declared_price_reaches_the_rates_of_a_provider_loaded_from_its_folder() {
    let root = std::env::temp_dir().join(format!("declared-prices-{}", uuid::Uuid::new_v4()));
    let folder = root.join("providers/cheap");
    std::fs::create_dir_all(&folder).unwrap_or_else(|error| panic!("{error}"));
    let declared = r#"import { usd } from "sdk/providers/index.ts";
export function provider() {
  return { id: "cheap", name: "Cheap", laterDiscount: 0.5, searchPrice: 0.02, priceDate: "2026-01-02", basis: "api",
    prices: { "tiny-1": { ...usd(1, 2, 0.1, { short: 1.25, long: 2 }), laterDiscount: 0.75 } } };
}"#;
    std::fs::write(folder.join("index.ts"), declared).unwrap_or_else(|error| panic!("{error}"));
    let loaded = crate::scripted::ScriptedProvider::load_in(Some(&root), "providers/cheap/index.ts");
    let _ = std::fs::remove_dir_all(&root);
    let cheap = loaded.unwrap_or_else(|error| panic!("{error}"));
    let rates = crate::priced(&cheap, "cheap:tiny-1", 0, Cache::Long).expect("priced");
    assert_eq!((rates.input, rates.output, rates.cache_write), (1_000_000, 2_000_000, 2_000_000));
    assert_eq!(crate::Provider::search_price(&cheap), Some(20_000));
    assert_eq!(crate::later_discount(&cheap, "tiny-1"), Some(0.75));
    assert_eq!(crate::later_discount(&cheap, "other"), Some(0.5));
    assert_eq!(crate::Provider::price_date(&cheap), "2026-01-02");
    assert_eq!(crate::Provider::basis(&cheap), domain::billing::Basis::Api);
}
