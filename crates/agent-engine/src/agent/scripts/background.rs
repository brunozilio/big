use serde_json::{Value, json};

use super::agents::answer;
use super::{Script, Within};
use crate::agent::Agent;

pub(super) const SPACE: &str = "background";

pub(super) fn call<'env>(
    _agent: &'env Agent<'_>,
    within: Within<'_, 'env>,
    _script: &Script<'_>,
    name: &str,
    arguments: &Value,
) -> Result<Value, String> {
    let background = &within.run.background;
    let id = arguments["task_id"].as_str().unwrap_or_default();
    match name {
        "output" => {
            let Some(task) = background.find(id) else { return Ok(Value::Null) };
            Ok(match task.waited(arguments["block"].as_bool().unwrap_or(true)) {
                Some(outcome) => json!({ "done": true, "report": answer(&outcome) }),
                None => json!({ "done": false }),
            })
        }
        "stop" => Ok(Value::Bool(background.stop(id))),
        "list" => Ok(Value::Array(background.listed())),
        other => Err(format!("ctx.{SPACE}.{other} is not a function")),
    }
}
