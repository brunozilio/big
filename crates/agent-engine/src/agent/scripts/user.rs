use domain::{AskKind, Decision, Question};
use serde_json::{Value, json};

use super::{Script, Within};
use crate::agent::Agent;

pub(super) const SPACE: &str = "user";

pub(super) fn call<'env>(
    agent: &'env Agent<'_>,
    within: Within<'_, 'env>,
    _script: &Script<'_>,
    name: &str,
    arguments: &Value,
) -> Result<Value, String> {
    match name {
        "prompts" => Ok(Value::Bool(!agent.control.no_prompts())),
        "ask" => ask(agent, within.id, arguments),
        other => Err(format!("ctx.{SPACE}.{other} is not a function")),
    }
}

fn ask(agent: &Agent<'_>, id: &str, arguments: &Value) -> Result<Value, String> {
    if agent.control.no_prompts() {
        return Err("Interactive questions are disabled for this agent.".into());
    }
    let asked: Vec<Question> = serde_json::from_value(arguments["questions"].clone())
        .map_err(|error| format!("the questions do not read: {error}"))?;
    agent.notify("elicitation_dialog", concat!(brand::name!(), " has questions for you"));
    Ok(match agent.asked(id, AskKind::Questions(asked)) {
        Ok(Decision::Answer(picks)) => json!({ "answered": picks }),
        Ok(Decision::Allow | Decision::AllowAlways | Decision::Approve(_)) => json!({ "answered": [] }),
        Ok(Decision::Deny) => json!({ "declined": true }),
        Ok(Decision::DenyWith(note)) => json!({ "declined": true, "note": note }),
        Err(_) => json!({ "stopped": true }),
    })
}
