use serde_json::Value;

pub fn matches(pattern: &Value, value: &Value) -> bool {
    match pattern {
        Value::Null => true,
        Value::Object(fields) => fields.iter().all(|(key, wanted)| match value.get(key) {
            Some(found) => matches(wanted, found) || wanted.is_null() && found.is_null(),
            None => wanted.is_null(),
        }),
        Value::Array(options) => match value {
            Value::Array(_) if options.iter().all(|option| !option.is_array()) && pattern == value => true,
            _ => options.iter().any(|option| matches(option, value)),
        },
        other => other == value,
    }
}

pub fn event_matches(filter: &Value, event: &Value) -> bool {
    let mut merged =
        event.get("data").filter(|data| data.is_object()).cloned().unwrap_or_else(|| serde_json::json!({}));
    if let (Some(fields), Some(top)) = (merged.as_object_mut(), event.as_object()) {
        for (key, value) in top {
            if key != "data" {
                fields.entry(key.clone()).or_insert_with(|| value.clone());
            }
        }
    }
    matches(filter, &merged) || matches(filter, event)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn a_pattern_matches_partially_recursively_and_arrays_mean_any_of() {
        let run = json!({"ref": "r1", "state": "passed", "jobs": [{"id": 1}]});
        assert!(matches(&json!({"state": "passed"}), &run));
        assert!(matches(&json!({"state": ["failed", "passed"]}), &run));
        assert!(!matches(&json!({"state": ["failed", "cancelled"]}), &run));
        assert!(!matches(&json!({"state": "passed", "ref": "r2"}), &run));
        assert!(matches(&Value::Null, &run));
    }

    #[test]
    fn an_event_filter_looks_at_the_data_and_the_envelope() {
        let event = json!({"kind": "pipelines.run.finished", "subject": "r1", "data": {"run": "r1", "to": "Feito"}});
        assert!(event_matches(&json!({"run": "r1"}), &event));
        assert!(event_matches(&json!({"subject": "r1", "to": "Feito"}), &event));
        assert!(!event_matches(&json!({"run": "r2"}), &event));
    }
}
