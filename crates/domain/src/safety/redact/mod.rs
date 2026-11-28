mod known;
mod rules;
mod scan;
mod shared;

pub use known::Known;
pub use rules::{ENTROPY, Engine, Entropy, JWT, Pattern, Prefix, Rules};
pub use scan::{ASSIGNMENT, KNOWN, LABEL, Redaction};
pub use shared::{current, declare, shipped};

pub const REDACTED: &str = "[redacted]";
pub const FILE: &str = "safety/redaction.json";

pub fn redact(text: &str, known: &Known) -> String {
    current().redact(text, known).text
}

pub fn found(text: &str, known: &Known) -> Vec<String> {
    current().redact(text, known).found
}

pub(crate) fn words(text: &str) -> Vec<String> {
    current().words(text)
}

#[cfg(test)]
mod tests;
