use serde_json::Value;

use super::{Redactor, Rules};

const SCHEMA_VERSION: u64 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Problem {
    pub severity: Severity,
    pub text: String,
}

impl Problem {
    fn error(text: String) -> Problem {
        Problem { severity: Severity::Error, text }
    }

    fn warning(text: String) -> Problem {
        Problem { severity: Severity::Warning, text }
    }
}

impl std::fmt::Display for Problem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.text)
    }
}

pub fn layer(text: &str) -> Result<Rules, String> {
    let value: Value = serde_json::from_str(text).map_err(|error| error.to_string())?;
    let version = value.get("schema_version").and_then(Value::as_u64).unwrap_or(SCHEMA_VERSION);
    if version > SCHEMA_VERSION {
        return Err(format!("schema_version {version} is newer than the {SCHEMA_VERSION} this version reads"));
    }
    Rules::read(&value)
}

pub fn shipped(text: Option<&str>) -> (Rules, Option<Problem>) {
    match text.map(layer) {
        Some(Ok(rules)) => (rules, None),
        Some(Err(error)) => (Rules::default(), Some(Problem::error(format!("the shipped rules do not load: {error}")))),
        None => (Rules::default(), Some(Problem::error("the shipped rules are missing".into()))),
    }
}

pub fn assemble(shipped_text: Option<&str>, mine: Option<&str>, connectors: &[Rules]) -> (Redactor, Vec<Problem>) {
    let (mut rules, problem) = shipped(shipped_text);
    let mut problems: Vec<Problem> = problem.into_iter().collect();
    if let Some(text) = mine {
        match layer(text) {
            Ok(added) => rules.merge(&added),
            Err(error) => problems.push(Problem::warning(format!("the user's rules do not load: {error}"))),
        }
    }
    for declared in connectors {
        rules.merge(&declared.shapes());
    }
    let (redactor, left_out) = Redactor::compiled(rules);
    problems.extend(left_out.into_iter().map(|text| Problem::warning(format!("a pattern was left out: {text}"))));
    (redactor, problems)
}

pub fn widened(base: &Redactor, shipped_text: Option<&str>, mine: Option<&str>) -> (Redactor, Vec<Problem>) {
    let mut rules = base.rules().clone();
    let mut problems = Vec::new();
    for (whose, text) in [("shipped", shipped_text), ("user's", mine)] {
        match text.map(layer) {
            Some(Ok(added)) => rules.merge(&added),
            Some(Err(error)) => problems.push(Problem::warning(format!("the {whose} rules do not load: {error}"))),
            None => {}
        }
    }
    let (redactor, left_out) = Redactor::compiled(rules);
    problems.extend(left_out.into_iter().map(|text| Problem::warning(format!("a pattern was left out: {text}"))));
    (redactor, problems)
}
