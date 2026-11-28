use regex::Regex;
use serde::Deserialize;
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Prefix {
    pub name: String,
    pub prefix: String,
    #[serde(default)]
    pub min: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Pattern {
    pub name: String,
    pub regex: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Entropy {
    pub min_length: usize,
    pub bits: f64,
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Rules {
    pub prefixes: Vec<Prefix>,
    pub patterns: Vec<Pattern>,
    pub keys: Vec<String>,
    pub labels: Vec<String>,
    pub parse_errors: Vec<String>,
    pub jwt: bool,
    pub entropy: Option<Entropy>,
}

fn joined<T: PartialEq + Clone>(mine: &mut Vec<T>, more: &[T]) {
    for item in more {
        if !mine.contains(item) {
            mine.push(item.clone());
        }
    }
}

impl Rules {
    pub fn read(value: &Value) -> Result<Rules, String> {
        Rules::deserialize(value).map_err(|error| error.to_string())
    }

    pub fn merge(&mut self, more: &Rules) {
        joined(&mut self.prefixes, &more.prefixes);
        joined(&mut self.patterns, &more.patterns);
        joined(&mut self.keys, &more.keys);
        joined(&mut self.labels, &more.labels);
        joined(&mut self.parse_errors, &more.parse_errors);
        self.jwt |= more.jwt;
        self.entropy = match (self.entropy, more.entropy) {
            (Some(mine), Some(theirs)) => Some(if theirs.bits < mine.bits { theirs } else { mine }),
            (mine, theirs) => mine.or(theirs),
        };
    }

    pub fn layered<'a>(layers: impl IntoIterator<Item = &'a Value>) -> (Rules, Vec<String>) {
        let mut rules = Rules::default();
        let mut problems = Vec::new();
        for value in layers {
            match Rules::read(value) {
                Ok(read) => rules.merge(&read),
                Err(problem) => problems.push(problem),
            }
        }
        (rules, problems)
    }
}

#[derive(Debug)]
pub struct Engine {
    rules: Rules,
    regexes: Vec<(String, Regex)>,
}

pub const JWT: &str = "jwt";
pub const ENTROPY: &str = "entropy";

impl Engine {
    pub fn compiled(rules: Rules) -> (Engine, Vec<String>) {
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
        (Engine { rules, regexes }, problems)
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
