mod assemble;
mod known;
mod redactor;
mod rules;
mod scan;

pub use assemble::{Problem, Severity, assemble, layer, shipped};
pub use known::Known;
pub use redactor::{ENTROPY, JWT, Redactor};
pub use rules::{Entropy, Pattern, Prefix, Rules};
pub use scan::{ASSIGNMENT, KNOWN, LABEL, Redaction};

pub const REDACTED: &str = "[redacted]";
pub const FILE: &str = "safety/redaction.json";

#[cfg(test)]
pub(crate) fn testing() -> Redactor {
    let rules = serde_json::json!({
        "prefixes": [{ "name": "house", "prefix": "house_", "min": 12 }],
        "keys": ["password", "secret", "token", "api_key"],
        "labels": ["bearer"],
        "parseErrors": ["unexpected", "invalid"],
        "typeNames": ["string", "number"],
        "valueMin": 6,
        "jwt": true
    });
    Redactor::compiled(Rules::read(&rules).unwrap_or_default()).0
}

#[cfg(test)]
mod tests;
