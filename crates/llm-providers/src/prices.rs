use domain::billing::TokenRates;

const SOURCES: &str = "pricing";
const TABLE: &str = "prices_";
const MICRO: f64 = 1_000_000.0;

pub fn source(provider: &str) -> Option<&'static str> {
    lookup(brand::section(SOURCES), provider)
}

pub fn listed(source: &str, model: &str, prompt_tokens: u64) -> Option<TokenRates> {
    let table = brand::section(&format!("{TABLE}{}", key(source)));
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
    let amounts: Vec<u64> =
        words.map(|word| word.parse::<f64>().ok().map(|usd| (usd * MICRO).round() as u64)).collect::<Option<_>>()?;
    match amounts[..] {
        [input, output, cache_read, cache_write] => Some(TokenRates { input, output, cache_read, cache_write }),
        _ => None,
    }
}

#[cfg(test)]
#[path = "prices_tests.rs"]
mod tests;
