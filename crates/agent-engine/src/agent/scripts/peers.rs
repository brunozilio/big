use serde_json::{Value, json};

use super::agents::answer;
use super::{Script, Within};
use crate::agent::Agent;
use crate::tools::peers::SessionMessage;

pub(super) const SPACE: &str = "peers";

pub(super) fn call<'env>(
    agent: &'env Agent<'_>,
    within: Within<'_, 'env>,
    _script: &Script<'_>,
    name: &str,
    arguments: &Value,
) -> Result<Value, String> {
    match name {
        "team" => Ok(match agent.team_message(arguments) {
            None => Value::Null,
            Some(Ok(to)) => json!({ "sent": to }),
            Some(Err(problem)) => json!({ "problem": problem }),
        }),
        "session" => {
            if !agent.to_session(arguments, &within.run.conversations) {
                return Ok(Value::Null);
            }
            let subject = arguments["to"].as_str().unwrap_or_default();
            let sent = agent.permitted(&within.permitting(subject, None), &SessionMessage, arguments);
            Ok(answer(&sent))
        }
        other => Err(format!("ctx.{SPACE}.{other} is not a function")),
    }
}
