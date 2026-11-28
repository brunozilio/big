use domain::permission::{AskOutcome, Reason, Surface};
use serde_json::{Value, json};

pub const FILE: &str = "prompts/outcomes.js";
pub const FOLDER: &str = "prompts/outcomes";

#[derive(Debug, Clone, Copy)]
pub struct Told<'a> {
    pub outcome: AskOutcome,
    pub surface: Surface,
    pub tool: &'a str,
    pub operation: Option<&'a str>,
    pub reason: Option<&'a Reason>,
    pub why: Option<&'a str>,
    pub note: Option<&'a str>,
}

impl<'a> Told<'a> {
    pub fn new(outcome: AskOutcome, surface: Surface, tool: &'a str) -> Self {
        Self { outcome, surface, tool, operation: None, reason: None, why: None, note: None }
    }

    pub fn operation(self, operation: &'a str) -> Self {
        Self { operation: Some(operation), ..self }
    }

    pub fn reason(self, reason: &'a Reason) -> Self {
        Self { reason: Some(reason), ..self }
    }

    pub fn why(self, why: &'a str) -> Self {
        Self { why: Some(why), ..self }
    }

    pub fn note(self, note: &'a str) -> Self {
        Self { note: Some(note), ..self }
    }

    fn data(&self) -> Value {
        json!({
            "outcome": self.outcome.word(),
            "surface": self.surface.word(),
            "name": self.tool,
            "operation": self.operation.unwrap_or(self.tool),
            "reason": self.reason.map(|reason| json!({ "kind": reason.kind(), "why": reason.why() })),
            "why": self.why,
            "note": self.note,
        })
    }
}

pub fn told(told: &Told<'_>) -> String {
    told_in(content::folder().as_deref(), told)
}

pub fn told_in(folder: Option<&std::path::Path>, told: &Told<'_>) -> String {
    told_from(folder, FILE, told)
}

fn told_from(folder: Option<&std::path::Path>, general: &str, told: &Told<'_>) -> String {
    let data = content::with_brand(&told.data());
    let operation = told.operation.unwrap_or(told.tool);
    let own = file_of(operation).and_then(|file| content::call_in(folder, &file, "default", &data)).and_then(text);
    own.or_else(|| content::call_in(folder, general, "default", &data).and_then(text))
        .unwrap_or_else(|| told.tool.to_owned())
}

fn file_of(operation: &str) -> Option<String> {
    let plain = operation.chars().all(|letter| letter.is_ascii_alphanumeric() || matches!(letter, '.' | '_' | '-'));
    (plain && !operation.is_empty() && !operation.starts_with('.')).then(|| format!("{FOLDER}/{operation}.js"))
}

fn text(value: Value) -> Option<String> {
    value.as_str().map(str::trim).filter(|told| !told.is_empty()).map(str::to_owned)
}

#[cfg(test)]
mod tests;
