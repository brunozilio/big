use regex::Regex;

use super::{Entropy, Rules};

pub const JWT: &str = "jwt";
pub const ENTROPY: &str = "entropy";

#[derive(Debug)]
pub struct Redactor {
    rules: Rules,
    regexes: Vec<(String, Regex)>,
}

impl Default for Redactor {
    fn default() -> Self {
        Redactor::compiled(Rules::default()).0
    }
}

impl Redactor {
    pub fn compiled(rules: Rules) -> (Redactor, Vec<String>) {
        let mut problems = Vec::new();
        let regexes = rules
            .patterns
            .iter()
            .filter_map(|pattern| match Regex::new(&pattern.regex) {
                Ok(regex) => Some((pattern.name.clone(), regex)),
                Err(error) => {
                    problems.push(format!("{}: {error}", pattern.name));
                    None
                }
            })
            .collect();
        (Redactor { rules, regexes }, problems)
    }

    pub fn rules(&self) -> &Rules {
        &self.rules
    }

    pub fn token(&self, word: &str) -> Option<&str> {
        let prefixed = self.rules.prefixes.iter().find(|rule| word.starts_with(&rule.prefix) && word.len() >= rule.min);
        if let Some(rule) = prefixed {
            return Some(&rule.name);
        }
        if let Some((name, _)) = self.regexes.iter().find(|(_, regex)| regex.is_match(word)) {
            return Some(name);
        }
        if self.rules.jwt && is_jwt(word) {
            return Some(JWT);
        }
        self.rules.entropy.filter(|entropy| random(word, *entropy)).map(|_| ENTROPY)
    }

    pub fn secret_key(&self, key: &str) -> bool {
        let key = key.to_ascii_lowercase();
        self.rules.keys.iter().any(|secret| key.ends_with(&secret.to_ascii_lowercase()))
    }

    pub fn label(&self, word: &str) -> bool {
        self.rules.labels.iter().any(|label| word.eq_ignore_ascii_case(label))
    }

    pub fn parse_error(&self, word: &str) -> bool {
        self.rules.parse_errors.iter().any(|error| word.eq_ignore_ascii_case(error))
    }

    pub fn secret_value(&self, value: &str) -> bool {
        let long = value.chars().count() >= self.rules.value_min.unwrap_or(1).max(1);
        long && !self.rules.type_names.iter().any(|name| value.eq_ignore_ascii_case(name))
    }
}

fn is_jwt(word: &str) -> bool {
    let parts: Vec<&str> = word.split('.').collect();
    parts.len() == 3
        && word.starts_with("eyJ")
        && parts.iter().all(|p| p.len() >= 4 && p.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'))
}

fn random(word: &str, entropy: Entropy) -> bool {
    let length = word.chars().count();
    if length < entropy.min_length.max(1) {
        return false;
    }
    let letters = word.chars().any(|c| c.is_ascii_alphabetic());
    let digits = word.chars().any(|c| c.is_ascii_digit());
    letters && digits && bits(word) >= entropy.bits
}

fn bits(word: &str) -> f64 {
    let mut counts = std::collections::BTreeMap::new();
    for c in word.chars() {
        *counts.entry(c).or_insert(0_u32) += 1;
    }
    let total = f64::from(u32::try_from(word.chars().count()).unwrap_or(u32::MAX));
    counts
        .values()
        .map(|count| {
            let share = f64::from(*count) / total;
            -share * share.log2()
        })
        .sum()
}
