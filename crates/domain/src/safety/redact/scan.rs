use serde_json::Value;

use super::{Known, REDACTED, Redactor};

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
    engine: &'a Redactor,
    next: bool,
    found: Vec<String>,
}

enum Hit<'a> {
    Core(&'a str),
    After(usize),
}

impl<'a> Scan<'a> {
    fn saw(&mut self, name: &str) {
        if !self.found.iter().any(|known| known == name) {
            self.found.push(name.to_owned());
        }
    }

    fn hit(&mut self, word: &str) -> Option<Hit<'a>> {
        let core = core(word);
        let engine = self.engine;
        if std::mem::take(&mut self.next) && engine.labelled_value(core) {
            return Some(Hit::Core(LABEL));
        }
        if let Some(name) = engine.token(core) {
            return Some(Hit::Core(name));
        }
        if let Some(at) = self.assignment(word) {
            return Some(Hit::After(at));
        }
        self.next = engine.label(core) || self.secret_label(word);
        None
    }

    fn word(&mut self, word: &str) -> String {
        match self.hit(word) {
            Some(Hit::Core(name)) => {
                self.saw(name);
                replace_core(word)
            }
            Some(Hit::After(at)) => {
                self.saw(ASSIGNMENT);
                format!("{}{REDACTED}", &word[..at])
            }
            None => word.to_owned(),
        }
    }

    fn assignment(&self, word: &str) -> Option<usize> {
        let at = word.find(['=', ':'])?;
        let (key, value) = (&word[..at], &word[at + 1..]);
        (self.engine.secret_value(core(value)) && self.engine.secret_key(core(key))).then_some(at + 1)
    }

    fn secret_label(&self, word: &str) -> bool {
        word.strip_suffix(':').is_some_and(|key| self.engine.secret_key(core(key)))
    }

    fn names_a_parse_error(&self, previous: &str, label: &str) -> bool {
        self.engine.parse_error(core(previous)) && core(label).eq_ignore_ascii_case("token:")
    }
}

impl Redactor {
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

    pub fn finds(&self, text: &str, known: &Known) -> bool {
        let mut scan = self.scan();
        let mut previous = "";
        for piece in text.split_whitespace() {
            if scan.hit(piece).is_some() {
                return true;
            }
            scan.next &= !scan.names_a_parse_error(previous, piece);
            previous = piece;
        }
        known.values().iter().any(|value| text.contains(value.as_str()))
    }

    pub fn finds_in(&self, value: &Value, known: &Known) -> bool {
        match value {
            Value::String(text) => self.finds(text, known),
            Value::Array(items) => items.iter().any(|item| self.finds_in(item, known)),
            Value::Object(fields) => fields.values().any(|item| self.finds_in(item, known)),
            _ => false,
        }
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
