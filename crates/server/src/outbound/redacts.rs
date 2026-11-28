use domain::redact::{Known, REDACTED, Redactor, Rules};
use serde_json::Value;

pub trait Redacts: Send + Sync {
    fn rules(&self) -> &Rules;
    fn text(&self, text: &str) -> String;
    fn value(&self, key: &str, value: &Value) -> Value;
}

impl Redacts for Redactor {
    fn rules(&self) -> &Rules {
        Redactor::rules(self)
    }

    fn text(&self, text: &str) -> String {
        self.redact(text, &Known::default()).text
    }

    fn value(&self, key: &str, value: &Value) -> Value {
        match value {
            Value::String(text) if !text.is_empty() && self.secret_key(key) => Value::String(REDACTED.to_owned()),
            other => self.redact_value(other, &Known::default()).0,
        }
    }
}
