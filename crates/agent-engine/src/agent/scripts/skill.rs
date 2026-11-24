use serde_json::{Value, json};

use super::{Script, Within};
use crate::agent::Agent;

pub(super) const SPACE: &str = "skill";

pub(super) fn call<'env>(
    agent: &'env Agent<'_>,
    _within: Within<'_, 'env>,
    _script: &Script<'_>,
    name: &str,
    arguments: &Value,
) -> Result<Value, String> {
    let mut skills = agent.tools.iter().flat_map(|tool| tool.skills());
    match name {
        "list" => Ok(Value::Array(
            skills.map(|skill| json!({ "name": skill.name, "modelInvocation": skill.model_invocation })).collect(),
        )),
        "load" => {
            let named = crate::tools::host::text(arguments, "name")?;
            let skill =
                skills.find(|skill| skill.name == named).ok_or_else(|| format!("there is no skill named {named}"))?;
            let args = arguments.get("args").and_then(Value::as_str).unwrap_or_default();
            let loaded = crate::tools::skill::load_for(skill, args, agent.workspace);
            Ok(json!({ "content": loaded.content, "isError": loaded.is_error }))
        }
        other => Err(format!("ctx.{SPACE}.{other} is not a function")),
    }
}
