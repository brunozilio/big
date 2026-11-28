use domain::permission::RuleKind as PermissionDecision;
use serde_json::Value;

use super::{CommandOutput, Handled, HookEvent, HookOutcome, failing, rewrites};

pub(super) fn absorbed(
    event: HookEvent,
    fail_closed: bool,
    result: CommandOutput,
    decision: &mut HookOutcome,
    input: &mut Value,
) -> Result<Handled, String> {
    if result.code == Some(2) {
        decision.blocked = Some(result.stderr);
        return Ok(Handled::Blocked);
    }
    if let Some(why) = failing::exited(event, fail_closed, &result) {
        decision.blocked = Some(why);
        return Ok(Handled::Blocked);
    }
    if result.code != Some(0) {
        rewrites::failed(event, &result.stderr, decision);
        return Ok(Handled::GoOn);
    }
    match serde_json::from_str::<Value>(&result.stdout) {
        Ok(value) => answered(event, &value, decision, input)?,
        Err(_) if event.takes_plain_text_as_context() => {
            decision.context.push_str(&result.stdout);
        }
        Err(_) => {}
    }
    Ok(if decision.blocked.is_some() { Handled::Blocked } else { Handled::GoOn })
}

fn answered(event: HookEvent, value: &Value, decision: &mut HookOutcome, input: &mut Value) -> Result<(), String> {
    let specific = &value["hookSpecificOutput"];
    if let Some(reason) = blocking(value) {
        decision.blocked = Some(reason);
    }
    permission_answered(event, specific, decision)?;
    if event == HookEvent::PostToolUse {
        decision.updated_output = specific.get("updatedToolOutput").cloned();
    }
    if decision.model.is_none() && matches!(event, HookEvent::ModelRequest | HookEvent::ModelError) {
        decision.model = specific["model"].as_str().filter(|m| !m.trim().is_empty()).map(Into::into);
    }
    if decision.effort.is_none() && event == HookEvent::ModelRequest {
        decision.effort = specific["effort"].as_str().and_then(domain::interception::effort);
    }
    rewrites::absorb(event, value, decision, input);
    if decision.text.is_none() && event == HookEvent::ModelResponse {
        decision.text = specific["text"].as_str().map(Into::into);
    }
    for text in [value["systemMessage"].as_str(), specific["additionalContext"].as_str()].into_iter().flatten() {
        decision.context.push_str(text);
        decision.context.push('\n');
    }
    input_updated(event, specific, decision, input)
}

fn blocking(value: &Value) -> Option<String> {
    let specific = &value["hookSpecificOutput"];
    let blocks = value["continue"] == false || value["decision"] == "block" || specific["permissionDecision"] == "deny";
    blocks.then(|| {
        value["reason"]
            .as_str()
            .or_else(|| value["stopReason"].as_str())
            .or_else(|| specific["permissionDecisionReason"].as_str())
            .unwrap_or("Blocked by configured hook")
            .into()
    })
}

fn permission_answered(event: HookEvent, specific: &Value, decision: &mut HookOutcome) -> Result<(), String> {
    let permission = if event == HookEvent::PermissionRequest {
        specific["decision"]["behavior"].as_str()
    } else {
        specific["permissionDecision"].as_str()
    };
    let Some(permission) = permission else { return Ok(()) };
    let permission = PermissionDecision::named(permission).ok_or("invalid permission decision from hook")?;
    decision.permission = Some(permission);
    if permission == PermissionDecision::Deny {
        let reason = specific["permissionDecisionReason"].as_str().unwrap_or("Permission hook denied the action");
        decision.blocked = Some(reason.into());
    }
    Ok(())
}

fn input_updated(
    event: HookEvent,
    specific: &Value,
    decision: &mut HookOutcome,
    input: &mut Value,
) -> Result<(), String> {
    let updated = match event {
        HookEvent::PermissionRequest => specific["decision"].get("updatedInput"),
        HookEvent::PreToolUse => specific.get("updatedInput"),
        _ => None,
    };
    let Some(updated) = updated else { return Ok(()) };
    if !updated.is_object() {
        return Err("hook updatedInput must be an object".into());
    }
    decision.updated_input = Some(updated.clone());
    input["tool_input"] = updated.clone();
    Ok(())
}
