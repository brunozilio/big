use serde_json::Value;

pub fn state(status: &Value, conclusion: &Value) -> &'static str {
    match conclusion.as_str() {
        Some("success") => return "passed",
        Some("failure" | "timed_out" | "startup_failure" | "action_required") => return "failed",
        Some("cancelled") => return "cancelled",
        Some("skipped" | "neutral" | "stale") => return "skipped",
        _ => {}
    }
    match status.as_str() {
        Some("waiting") => "waiting",
        Some("in_progress") => "running",
        Some("completed") => "failed",
        _ => "queued",
    }
}

pub fn verdict(state: &Value) -> &'static str {
    match state.as_str() {
        Some("APPROVED") => "approve",
        Some("CHANGES_REQUESTED") => "request_changes",
        Some("DISMISSED") => "dismissed",
        Some("PENDING") => "pending",
        _ => "comment",
    }
}

pub fn change(status: &Value) -> &'static str {
    match status.as_str() {
        Some("added") => "added",
        Some("removed") => "removed",
        Some("renamed" | "copied") => "renamed",
        _ => "modified",
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn the_provider_s_states_become_the_contract_s_states() {
        assert_eq!(state(&json!("completed"), &json!("success")), "passed");
        assert_eq!(state(&json!("completed"), &json!("timed_out")), "failed");
        assert_eq!(state(&json!("completed"), &json!("neutral")), "skipped");
        assert_eq!(state(&json!("waiting"), &Value::Null), "waiting");
        assert_eq!(state(&json!("in_progress"), &Value::Null), "running");
        assert_eq!(state(&json!("requested"), &Value::Null), "queued");
        assert_eq!(verdict(&json!("CHANGES_REQUESTED")), "request_changes");
        assert_eq!(change(&json!("copied")), "renamed");
    }
}
