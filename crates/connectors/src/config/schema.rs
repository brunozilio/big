use std::collections::BTreeMap;

use serde::Deserialize;
use serde_json::{Map, Value};

pub const EXPORT: &str = "config";

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
enum Kind {
    #[default]
    String,
    Integer,
    Number,
    Boolean,
    List,
}

impl Kind {
    fn named(self) -> &'static str {
        match self {
            Self::String => "a text",
            Self::Integer => "a whole number",
            Self::Number => "a number",
            Self::Boolean => "true or false",
            Self::List => "a list of words",
        }
    }

    fn read(self, value: &Value) -> Option<Value> {
        let text = value.as_str().map(str::trim);
        match self {
            Self::String => text.map(Value::from),
            Self::Integer => value.as_i64().or_else(|| text?.parse().ok()).map(Value::from),
            Self::Number => value.as_f64().or_else(|| text?.parse().ok()).map(Value::from),
            Self::Boolean => value.as_bool().or_else(|| text?.parse().ok()).map(Value::from),
            Self::List => match value {
                Value::Array(items) => items.iter().all(Value::is_string).then(|| value.clone()),
                _ => text.map(|text| text.split_whitespace().map(Value::from).collect()),
            },
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct Field {
    #[serde(rename = "type", default)]
    kind: Kind,
    #[serde(default)]
    required: bool,
    #[serde(default)]
    default: Option<Value>,
    #[serde(default)]
    choices: Vec<String>,
    #[serde(default, rename = "description")]
    _description: String,
}

impl Field {
    fn checked(&self, key: &str, value: &Value) -> Result<Value, String> {
        let read =
            self.kind.read(value).ok_or_else(|| format!("`{key}` must be {}, not {value}", self.kind.named()))?;
        let chosen = |text: &str| self.choices.iter().any(|choice| choice == text);
        let words: Vec<&str> = match &read {
            Value::String(text) => vec![text.as_str()],
            Value::Array(items) => items.iter().filter_map(Value::as_str).collect(),
            _ => Vec::new(),
        };
        match words.iter().find(|word| !self.choices.is_empty() && !chosen(word)) {
            Some(word) => Err(format!("`{key}` is `{word}`, not one of {}", self.choices.join(", "))),
            None => Ok(read),
        }
    }
}

fn fields(section: &str, schema: &Value) -> Result<BTreeMap<String, Field>, String> {
    let fields: BTreeMap<String, Field> = serde_json::from_value(schema.clone())
        .map_err(|error| format!("{section}: the `{EXPORT}` export is not a valid configuration schema: {error}"))?;
    for (key, field) in &fields {
        if let Some(default) = &field.default {
            field
                .checked(key, default)
                .map_err(|problem| format!("{section}: the default of the `{EXPORT}` schema is wrong: {problem}"))?;
        }
    }
    Ok(fields)
}

pub fn applied(section: &str, schema: &Value, given: &Map<String, Value>) -> Result<Map<String, Value>, String> {
    let fields = fields(section, schema)?;
    let mut problems = Vec::new();
    let mut filled = Map::new();
    for key in given.keys().filter(|key| !fields.contains_key(*key)) {
        let known: Vec<&str> = fields.keys().map(String::as_str).collect();
        problems.push(format!("`{key}` is not one of the keys the connector reads ({})", known.join(", ")));
    }
    for (key, field) in &fields {
        match given.get(key).or(field.default.as_ref()) {
            Some(value) => match field.checked(key, value) {
                Ok(read) => drop(filled.insert(key.clone(), read)),
                Err(problem) => problems.push(problem),
            },
            None if field.required => problems.push(format!("`{key}` is required")),
            None => {}
        }
    }
    if problems.is_empty() {
        return Ok(filled);
    }
    Err(format!(
        "the [{section}] section of {} does not match what the connector declares: {}",
        super::FILE,
        problems.join("; ")
    ))
}
