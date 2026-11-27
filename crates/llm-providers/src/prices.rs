use std::collections::BTreeMap;

use domain::billing::TokenRates;
use serde::{Deserialize, Deserializer};

use crate::model::intent::Cache;

const TABLE: &str = "prices_";
const MICRO: f64 = 1_000_000.0;
const SHORT: &str = "short";

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(untagged)]
pub enum CacheWrite {
    Every(f64),
    Lifetimes(BTreeMap<String, f64>),
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Rates {
    pub input: f64,
    pub output: f64,
    pub cache_read: f64,
    pub cache_write: CacheWrite,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Tier {
    pub above: u64,
    #[serde(flatten)]
    pub rates: Rates,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Price {
    #[serde(flatten)]
    pub rates: Rates,
    #[serde(default)]
    pub tiers: Vec<Tier>,
    #[serde(default)]
    pub later_discount: Option<f64>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Prices(Vec<(String, Price)>);

pub static NONE: Prices = Prices(Vec::new());

impl<'de> Deserialize<'de> for Prices {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let listed = BTreeMap::<String, Price>::deserialize(deserializer)?;
        Ok(Prices(listed.into_iter().map(|(name, price)| (key(&name), price)).collect()))
    }
}

impl Prices {
    pub fn price(&self, model: &str) -> Option<&Price> {
        names(model).find_map(|name| {
            let wanted = key(name);
            self.0.iter().find(|(known, _)| *known == wanted).map(|(_, price)| price)
        })
    }

    pub fn rates(&self, model: &str, prompt_tokens: u64, cache: Cache) -> Option<TokenRates> {
        self.price(model).map(|price| price.rates(prompt_tokens, cache))
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl Price {
    pub fn rates(&self, prompt_tokens: u64, cache: Cache) -> TokenRates {
        let chosen = self.tiers.iter().filter(|tier| prompt_tokens > tier.above).map(|tier| &tier.rates).next_back();
        chosen.unwrap_or(&self.rates).micro(cache)
    }
}

impl Rates {
    fn micro(&self, cache: Cache) -> TokenRates {
        let write = match &self.cache_write {
            CacheWrite::Every(usd) => *usd,
            CacheWrite::Lifetimes(by) => by
                .get(cache.lifetime())
                .or_else(|| by.get(SHORT))
                .or_else(|| by.values().next())
                .copied()
                .unwrap_or(0.0),
        };
        TokenRates {
            input: micro(self.input),
            output: micro(self.output),
            cache_read: micro(self.cache_read),
            cache_write: micro(write),
        }
    }
}

fn micro(usd: f64) -> u64 {
    (usd * MICRO).round() as u64
}

pub fn overridden(provider: &str, model: &str, prompt_tokens: u64) -> Option<TokenRates> {
    let table = brand::section(&format!("{TABLE}{}", key(provider)));
    let line = names(model).find_map(|name| lookup(table, name))?;
    tier(line, prompt_tokens)
}

fn names(model: &str) -> impl Iterator<Item = &str> {
    let bare = model.rsplit(':').next().unwrap_or(model);
    std::iter::successors(Some(bare), |name| {
        name.split_once('.').filter(|(lead, _)| lead.chars().all(|c| c.is_ascii_alphabetic())).map(|(_, rest)| rest)
    })
}

fn lookup(pairs: &'static [(&'static str, &'static str)], name: &str) -> Option<&'static str> {
    let wanted = key(name);
    pairs.iter().find(|(key, _)| *key == wanted).map(|(_, value)| *value)
}

fn key(name: &str) -> String {
    let bare = name.rsplit(':').next().unwrap_or(name);
    bare.chars().map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '_' }).collect()
}

fn tier(line: &str, prompt_tokens: u64) -> Option<TokenRates> {
    let mut chosen = None;
    for (at, part) in line.split('|').enumerate() {
        let mut words = part.split_whitespace();
        let above = if at == 0 { 0 } else { words.next()?.parse::<u64>().ok()? };
        let rates = rates(words)?;
        if prompt_tokens > above || at == 0 {
            chosen = Some(rates);
        }
    }
    chosen
}

fn rates<'a>(words: impl Iterator<Item = &'a str>) -> Option<TokenRates> {
    let amounts: Vec<u64> = words.map(|word| word.parse::<f64>().ok().map(micro)).collect::<Option<_>>()?;
    match amounts[..] {
        [input, output, cache_read, cache_write] => Some(TokenRates { input, output, cache_read, cache_write }),
        _ => None,
    }
}

#[cfg(test)]
#[path = "prices_tests.rs"]
mod tests;
