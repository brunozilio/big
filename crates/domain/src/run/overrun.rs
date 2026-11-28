use serde_json::Value;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum Overrun {
    #[default]
    Stop,
    Ask,
    Smaller(String),
}

impl Overrun {
    pub fn declared(value: &Value) -> Self {
        match value {
            Value::String(word) if word.trim() == "ask" => Self::Ask,
            Value::Object(fields) => fields
                .get("model")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|model| !model.is_empty())
                .map_or(Self::Stop, |model| Self::Smaller(model.to_owned())),
            _ => Self::Stop,
        }
    }

    pub const fn word(&self) -> &'static str {
        match self {
            Self::Stop => "stop",
            Self::Ask => "ask",
            Self::Smaller(_) => "smaller",
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn nothing_declared_or_anything_unknown_stops_as_before() {
        for value in [Value::Null, json!("stop"), json!("later"), json!(3), json!({}), json!({ "model": " " })] {
            assert_eq!(Overrun::declared(&value), Overrun::Stop, "{value}");
        }
    }

    #[test]
    fn a_step_asks_a_person_or_names_the_smaller_model_to_fall_to() {
        assert_eq!(Overrun::declared(&json!("ask")), Overrun::Ask);
        assert_eq!(Overrun::declared(&json!({ "model": "small" })), Overrun::Smaller("small".into()));
        assert_eq!(Overrun::Smaller("small".into()).word(), "smaller");
    }
}
