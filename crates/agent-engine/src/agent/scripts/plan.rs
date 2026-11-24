use domain::PermissionMode;
use serde_json::{Value, json};

use super::{Script, Within};
use crate::agent::Agent;

pub(super) const SPACE: &str = "plan";

pub(super) fn call<'env>(
    agent: &'env Agent<'_>,
    within: Within<'_, 'env>,
    _script: &Script<'_>,
    name: &str,
    arguments: &Value,
) -> Result<Value, String> {
    let main = agent.depth == 0;
    let planning = agent.control.mode() == PermissionMode::Plan;
    match name {
        "state" => Ok(json!({ "main": main, "planning": planning, "prompts": !agent.control.no_prompts() })),
        "exit" if main && planning => {
            let plan = crate::tools::host::text(arguments, "plan")?;
            Ok(agent.put_plan(within.id, plan))
        }
        "exit" => Err("only the main agent leaves plan mode, and only while planning".into()),
        other => Err(format!("ctx.{SPACE}.{other} is not a function")),
    }
}
