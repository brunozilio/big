use serde_json::{Value, json};

use super::{Script, Within};
use crate::agent::Agent;
use crate::agent::dispatch::in_loop::not_alongside;
use crate::tools::Outcome;
use crate::tools::repl::REPL_TOOL;

pub(super) const SPACE: &str = "repl";

pub(super) fn call<'env>(
    agent: &'env Agent<'_>,
    within: Within<'_, 'env>,
    _script: &Script<'_>,
    name: &str,
    arguments: &Value,
) -> Result<Value, String> {
    match (name, within.spawn) {
        ("eval", Some(spawn)) => Ok(answer(agent.repl(within.id, arguments, within.run, within.parent, spawn))),
        ("eval", None) => Ok(answer(not_alongside(REPL_TOOL))),
        (other, _) => Err(format!("ctx.{SPACE}.{other} is not a function")),
    }
}

fn answer(outcome: Outcome) -> Value {
    json!({ "content": outcome.content, "isError": outcome.is_error, "images": outcome.images })
}
