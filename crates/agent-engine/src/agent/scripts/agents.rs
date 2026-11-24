use serde_json::{Value, json};

use super::{Script, Within};
use crate::agent::Agent;
use crate::agent::dispatch::in_loop::not_alongside;
use crate::tools::Outcome;
use crate::tools::names::AGENT;

pub(super) const SPACE: &str = "agent";

pub(super) fn call<'env>(
    agent: &'env Agent<'_>,
    within: Within<'_, 'env>,
    _script: &Script<'_>,
    name: &str,
    arguments: &Value,
) -> Result<Value, String> {
    let conversations = &within.run.conversations;
    match name {
        "spawn" => Ok(answer(&agent.sub_agent_with(
            arguments,
            agent.control,
            agent.workspace,
            within.parent,
            conversations,
            None,
        ))),
        "background" => Ok(in_background(agent, within, arguments)),
        "teammate" => Ok(match agent.start_teammate(arguments) {
            Ok((name, team)) => json!({ "name": name, "team": team }),
            Err(problem) => answer(&Outcome::error(problem)),
        }),
        "message" => Ok(answer(&agent.send_message(arguments, conversations))),
        other => Err(format!("ctx.{SPACE}.{other} is not a function")),
    }
}

fn in_background<'env>(agent: &'env Agent<'_>, within: Within<'_, 'env>, arguments: &Value) -> Value {
    let Some(spawn) = within.spawn else { return answer(&not_alongside(AGENT)) };
    match agent.start_background(arguments, within.run, within.parent, spawn) {
        Ok(id) => json!({ "task_id": id }),
        Err(refused) => answer(&refused),
    }
}

pub(super) fn answer(outcome: &Outcome) -> Value {
    json!({ "content": outcome.content, "isError": outcome.is_error })
}
