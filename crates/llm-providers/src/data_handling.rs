use domain::data_policy::Handling;
use serde_json::Value;

const UNKNOWN: &str = "unknown";

fn region(value: &Value) -> Option<String> {
    value
        .as_str()
        .map(str::trim)
        .filter(|region| !region.is_empty() && !region.eq_ignore_ascii_case(UNKNOWN))
        .map(str::to_owned)
}

fn training(value: &Value) -> Option<bool> {
    match value.as_str().map(str::trim) {
        Some("yes") => Some(true),
        Some("no") => Some(false),
        _ => None,
    }
}

fn retention(value: &Value) -> Option<u32> {
    match value {
        Value::Number(days) => days.as_u64().and_then(|days| u32::try_from(days).ok()),
        Value::String(text) if text.trim() == "none" => Some(0),
        _ => None,
    }
}

pub fn read(declared_region: &Value, data_handling: &Value) -> Handling {
    Handling {
        region: region(declared_region),
        training: training(&data_handling["training"]),
        retention_days: retention(&data_handling["retention"]),
    }
}

pub fn over(base: &Handling, declared: &Value) -> Handling {
    let given = |value: &Value| !value.is_null();
    let data = &declared["dataHandling"];
    Handling {
        region: if given(&declared["region"]) { region(&declared["region"]) } else { base.region.clone() },
        training: if given(&data["training"]) { training(&data["training"]) } else { base.training },
        retention_days: if given(&data["retention"]) { retention(&data["retention"]) } else { base.retention_days },
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn unknown_and_missing_values_stay_unknown() {
        assert_eq!(
            read(&json!("unknown"), &json!({ "training": "unknown", "retention": "unknown" })),
            Handling::default()
        );
        assert_eq!(read(&Value::Null, &Value::Null), Handling::default());
    }

    #[test]
    fn declared_values_are_read() {
        let read = read(&json!("eu-central-1"), &json!({ "training": "no", "retention": 30 }));
        assert_eq!(
            read,
            Handling { region: Some("eu-central-1".into()), training: Some(false), retention_days: Some(30) }
        );
        assert_eq!(super::read(&json!("local"), &json!({ "retention": "none" })).retention_days, Some(0));
    }

    #[test]
    fn a_model_declaration_overrides_only_what_it_names() {
        let base = Handling { region: Some("us".into()), training: Some(false), retention_days: Some(30) };
        let model = over(&base, &json!({ "region": "eu", "dataHandling": { "retention": "unknown" } }));
        assert_eq!(model, Handling { region: Some("eu".into()), training: Some(false), retention_days: None });
        assert_eq!(over(&base, &Value::Null), base);
    }
}
