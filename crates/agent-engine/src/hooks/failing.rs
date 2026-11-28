use super::{CommandOutput, HookEvent};

pub fn fails_closed() -> bool {
    true
}

pub(super) enum Failed {
    Blocks(String),
    GoesOn(String),
    Stops(String),
}

pub(super) fn exited(event: HookEvent, fail_closed: bool, result: &CommandOutput) -> Option<String> {
    if result.code == Some(0) || result.code == Some(2) || !fail_closed || !event.decides() {
        return None;
    }
    let code = result.code.map_or_else(|| "no exit code".to_owned(), |code| format!("exit {code}"));
    Some(blocked(event, &format!("({code})"), result.stderr.trim()))
}

pub(super) fn errored(event: HookEvent, fail_closed: bool, error: String) -> Failed {
    match (event.decides(), fail_closed) {
        (false, _) => Failed::Stops(error),
        (true, true) => Failed::Blocks(blocked(event, "to run", error.trim())),
        (true, false) => Failed::GoesOn(error),
    }
}

fn blocked(event: HookEvent, how: &str, said: &str) -> String {
    let said = if said.is_empty() { String::new() } else { format!(": {said}") };
    format!(
        "A {event} hook failed {how}{said}. A failing decision hook blocks the action; fix the hook, or declare failClosed: false on it to let actions through when it fails."
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn exit(code: Option<i32>) -> CommandOutput {
        CommandOutput { code, stdout: String::new(), stderr: "crashed".into() }
    }

    #[test]
    fn only_a_decision_hook_that_did_not_declare_otherwise_blocks_when_it_fails() {
        let pre = HookEvent::PreToolUse;
        assert!(exited(pre, true, &exit(Some(1))).is_some_and(|why| why.contains("(exit 1): crashed")));
        assert!(exited(pre, true, &exit(None)).is_some_and(|why| why.contains("no exit code")));
        assert_eq!(exited(pre, true, &exit(Some(0))), None);
        assert_eq!(exited(pre, true, &exit(Some(2))), None, "exit 2 is the hook's own block");
        assert_eq!(exited(pre, false, &exit(Some(1))), None);
        assert_eq!(exited(HookEvent::Stop, true, &exit(Some(1))), None);
        assert!(matches!(errored(pre, true, "no model".into()), Failed::Blocks(why) if why.contains("no model")));
        assert!(matches!(errored(HookEvent::PermissionRequest, false, "x".into()), Failed::GoesOn(_)));
        assert!(matches!(errored(HookEvent::Stop, true, "x".into()), Failed::Stops(_)));
    }
}
