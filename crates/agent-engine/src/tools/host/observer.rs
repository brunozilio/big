use serde_json::Value;

use super::{Reach, text};
use crate::tools::inbox;

pub const SPACE: &str = "observer";

pub fn call(reach: &Reach<'_>, name: &str, arguments: &Value) -> Result<Value, String> {
    match name {
        "report" => report(reach, arguments),
        other => Err(format!("ctx.{SPACE}.{other} is not a function")),
    }
}

fn report(reach: &Reach<'_>, arguments: &Value) -> Result<Value, String> {
    let message = text(arguments, "message")?.trim();
    if message.is_empty() {
        return Err("message is empty".into());
    }
    let workspace = reach.workspace;
    inbox::push(workspace.credentials.client, domain::observer::report(&workspace.agent_name, message));
    Ok(Value::Null)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::super::Fence;
    use super::*;
    use crate::tools::Workspace;

    #[test]
    fn a_report_reaches_the_watched_agent_s_inbox_in_the_observer_s_name() {
        let mut workspace = Workspace::new(".");
        workspace.credentials.client = 97_901;
        workspace.agent_name = "critic".into();
        let reach = Reach { workspace: &workspace, fence: Fence::of(&[]), streams: None };
        assert_eq!(call(&reach, "report", &json!({ "message": " You skipped beta. " })), Ok(Value::Null));
        assert_eq!(inbox::take(97_901), [domain::observer::report("critic", "You skipped beta.")]);
        assert!(call(&reach, "report", &json!({ "message": "  " })).is_err());
        assert!(inbox::take(97_901).is_empty());
    }
}
