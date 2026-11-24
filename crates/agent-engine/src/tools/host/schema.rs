use serde_json::{Value, json};

use super::Reach;

pub const SPACE: &str = "schema";

pub fn call(reach: &Reach<'_>, name: &str, arguments: &Value) -> Result<Value, String> {
    match name {
        "validate" => validate(arguments),
        "output" => Ok(reach.workspace.services.output_schema.clone().unwrap_or(Value::Null)),
        other => Err(format!("ctx.{SPACE}.{other} is not a function")),
    }
}

fn validate(arguments: &Value) -> Result<Value, String> {
    let schema = arguments.get("schema").ok_or("schema is missing")?;
    let validator = jsonschema::validator_for(schema).map_err(|error| format!("invalid JSON schema: {error}"))?;
    let value = arguments.get("value").unwrap_or(&Value::Null);
    Ok(match validator.validate(value) {
        Ok(()) => json!({ "valid": true }),
        Err(error) => json!({ "valid": false, "error": error.to_string() }),
    })
}

#[cfg(test)]
mod tests {
    use super::super::Fence;
    use super::*;
    use crate::tools::Workspace;

    #[test]
    fn a_value_is_checked_against_a_schema_and_the_first_problem_is_said() {
        let workspace = Workspace::new(".");
        let reach = Reach { workspace: &workspace, fence: Fence::of(&[]), streams: None };
        let schema = json!({ "type": "object", "properties": { "score": { "type": "number", "maximum": 1 } } });
        let valid = call(&reach, "validate", &json!({ "schema": schema, "value": { "score": 1 } }));
        assert_eq!(valid, Ok(json!({ "valid": true })));
        let invalid = call(&reach, "validate", &json!({ "schema": schema, "value": { "score": 2 } })).expect("checked");
        assert_eq!(invalid["valid"], false);
        assert!(invalid["error"].as_str().is_some_and(|error| error.contains('2')), "{invalid}");
        let broken = call(&reach, "validate", &json!({ "schema": { "type": 7 }, "value": 1 }));
        assert!(broken.is_err_and(|error| error.starts_with("invalid JSON schema")));
        assert!(call(&reach, "check", &Value::Null).is_err());
    }
}
