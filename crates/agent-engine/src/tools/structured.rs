use serde_json::{Value, json};

use super::{Tool, Workspace, conditional, names};

pub fn offer(schema: Value, workspace: &mut Workspace) -> Result<Box<dyn Tool>, String> {
    jsonschema::validator_for(&schema).map_err(|e| format!("invalid output JSON schema: {e}"))?;
    let tool = conditional::tool(names::STRUCTURED_OUTPUT, &json!({ "schema": schema }))
        .ok_or_else(|| format!("{} is not shipped", names::STRUCTURED_OUTPUT))?;
    workspace.services.output_schema = Some(schema);
    Ok(Box::new(tool))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_nested_result_and_rejects_missing_and_extra_fields() {
        let mut workspace = Workspace::new(".");
        let schema = json!({"type":"object","properties":{"score":{"type":"number","minimum":0,"maximum":1}},"required":["score"],"additionalProperties":false});
        let tool = offer(schema, &mut workspace).expect("valid test fixture");
        assert_eq!(tool.summary(&json!({})), "Structured result");
        assert!(tool.effects(&json!({"result":{"score":1}}), &workspace).is_empty());
        let accepted = tool.run(&json!({"result":{"score":1}}), &workspace);
        assert_eq!((accepted.content.as_str(), accepted.is_error), ("Structured result accepted.", false));
        for invalid in [json!({}), json!({"score":2}), json!({"score":"1"}), json!({"score":1,"extra":true})] {
            let refused = tool.run(&json!({"result":invalid}), &workspace);
            assert!(refused.is_error && refused.content.starts_with("Invalid structured result: "), "{refused:?}");
        }
        assert_eq!(
            tool.run(&json!({"result":{"score":1},"more":1}), &workspace).content,
            "Pass exactly one field named result."
        );
    }

    #[test]
    fn a_schema_that_does_not_compile_is_refused() {
        let mut workspace = Workspace::new(".");
        let refused = offer(json!({"type": 7}), &mut workspace).err().unwrap_or_default();
        assert!(refused.starts_with("invalid output JSON schema"), "{refused}");
        assert!(workspace.services.output_schema.is_none());
    }
}
