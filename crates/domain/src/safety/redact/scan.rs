use serde_json::Value;

use super::{Engine, Known, REDACTED};

pub const ASSIGNMENT: &str = "assignment";
pub const LABEL: &str = "label";
pub const KNOWN: &str = "known";

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Redaction {
    pub text: String,
    pub found: Vec<String>,
}

impl Redaction {
    pub fn sensitive(&self) -> bool {
        !self.found.is_empty()
    }
}

struct Scan<'a> {
    engine: &'a Engine,
    next: bool,
    found: Vec<String>,
}

impl Scan<'_> {
    fn saw(&mut self, name: &str) {
        if !self.found.iter().any(|known| known == name) {
            self.found.push(name.to_owned());
        }
    }

    fn word(&mut self, word: &str) -> String {
        if std::mem::take(&mut self.next) {
            self.saw(LABEL);
            return replace_core(word);
        }
        let core = core(word);
        if let Some(name) = self.engine.token(core) {
            let name = name.to_owned();
            self.saw(&name);
            replace_core(word)
        } else if let Some(at) = self.assignment(word) {
            self.saw(ASSIGNMENT);
            format!("{}{REDACTED}", &word[..at])
        } else {
            self.next = self.engine.label(core) || self.secret_label(word);
            word.to_owned()
        }
    }

    fn assignment(&self, word: &str) -> Option<usize> {
        let at = word.find(['=', ':'])?;
        let (key, value) = (&word[..at], &word[at + 1..]);
        (!core(value).is_empty() && self.engine.secret_key(core(key))).then_some(at + 1)
    }

    fn secret_label(&self, word: &str) -> bool {
        word.strip_suffix(':').is_some_and(|key| self.engine.secret_key(core(key)))
    }

    fn names_a_parse_error(&self, previous: &str, label: &str) -> bool {
        self.engine.parse_error(core(previous)) && core(label).eq_ignore_ascii_case("token:")
    }
}

impl Engine {
    fn scan(&self) -> Scan<'_> {
        Scan { engine: self, next: false, found: Vec::new() }
    }

    pub fn redact(&self, text: &str, known: &Known) -> Redaction {
        let mut scan = self.scan();
        let mut out = String::with_capacity(text.len());
        let mut previous = "";
        let mut rest = text;
        while !rest.is_empty() {
            let split = rest.find(char::is_whitespace).unwrap_or(rest.len());
            let (piece, after) = rest.split_at(split);
            if !piece.is_empty() {
                out.push_str(&scan.word(piece));
                scan.next &= !scan.names_a_parse_error(previous, piece);
                previous = piece;
            }
            let gap = after.find(|c: char| !c.is_whitespace()).unwrap_or(after.len());
            out.push_str(&after[..gap]);
            rest = &after[gap..];
        }
        for value in known.values() {
            if out.contains(value.as_str()) {
                out = out.replace(value.as_str(), REDACTED);
                scan.saw(KNOWN);
            }
        }
        Redaction { text: out, found: scan.found }
    }

    pub fn words(&self, text: &str) -> Vec<String> {
        let mut scan = self.scan();
        text.split_whitespace().map(|word| scan.word(word)).collect()
    }

    pub fn redact_value(&self, value: &Value, known: &Known) -> (Value, Vec<String>) {
        let mut found = Vec::new();
        let redacted = self.value_in(value, known, &mut found);
        (redacted, found)
    }

    fn value_in(&self, value: &Value, known: &Known, found: &mut Vec<String>) -> Value {
        match value {
            Value::String(text) => {
                let redaction = self.redact(text, known);
                for name in redaction.found {
                    if !found.contains(&name) {
                        found.push(name);
                    }
                }
                Value::String(redaction.text)
            }
            Value::Array(items) => Value::Array(items.iter().map(|item| self.value_in(item, known, found)).collect()),
            Value::Object(fields) => Value::Object(
                fields.iter().map(|(key, item)| (key.clone(), self.value_in(item, known, found))).collect(),
            ),
            other => other.clone(),
        }
    }
}

fn is_edge(c: char) -> bool {
    matches!(c, '(' | ')' | '[' | ']' | '{' | '}' | '"' | '\'' | '`' | ',' | ';' | '<' | '>') || c == '.'
}

fn core(word: &str) -> &str {
    word.trim_matches(is_edge)
}

fn replace_core(word: &str) -> String {
    let core = core(word);
    if core.is_empty() {
        return word.to_owned();
    }
    let start = word.find(core).unwrap_or(0);
    format!("{}{REDACTED}{}", &word[..start], &word[start + core.len()..])
}
