use std::path::PathBuf;

use super::*;
use crate::PermissionMode;

const CWD: &str = "/work/rocket";

fn bounds() -> Bounds {
    Bounds::of(vec![PathBuf::from(CWD)], vec![PathBuf::from("/data/results")])
}

fn context<'a>(mode: PermissionMode, remembered: &'a Remembered, bounds: &'a Bounds) -> PolicyContext<'a> {
    PolicyContext { mode, ceiling: None, remembered, bounds, no_prompts: false }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Access {
    Reads,
    ReadsOutside,
    Edits,
    EditsGuarded,
    Executes,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Outcome {
    Allow,
    Ask,
    Deny,
}

fn expected(mode: PermissionMode, access: Access) -> Outcome {
    match (mode, access) {
        (_, Access::Reads) | (PermissionMode::Bypass, _) => Outcome::Allow,
        (_, Access::ReadsOutside) => Outcome::Ask,
        (PermissionMode::Plan, _) => Outcome::Deny,
        (PermissionMode::AcceptEdits, Access::Edits) => Outcome::Allow,
        _ => Outcome::Ask,
    }
}

fn outcome(verdict: &Verdict) -> Outcome {
    match verdict {
        Verdict::Allow => Outcome::Allow,
        Verdict::Ask { .. } => Outcome::Ask,
        Verdict::Deny(_) => Outcome::Deny,
    }
}

fn place(path: &str) -> Place {
    Place::at(path)
}

fn cases() -> Vec<(&'static str, &'static str, Vec<Effect>, Access)> {
    let settings = format!("/work/rocket/{}/settings.json", brand::DIR);
    vec![
        ("Read", "src/a.rs", vec![Effect::ReadPath(place("/work/rocket/src/a.rs"))], Access::Reads),
        ("Read", "/etc/hosts", vec![Effect::ReadPath(place("/etc/hosts"))], Access::ReadsOutside),
        ("Read", "saved", vec![Effect::ReadPath(place("/data/results/call_1.txt"))], Access::Reads),
        ("Grep", "todo", vec![Effect::ReadTree(place("/opt/lib"))], Access::ReadsOutside),
        ("Write", "src/a.rs", vec![Effect::WritePath(place("/work/rocket/src/a.rs"))], Access::Edits),
        ("Write", "/home/ana/.zshrc", vec![Effect::WritePath(place("/home/ana/.zshrc"))], Access::EditsGuarded),
        ("Edit", "config", vec![Effect::WritePath(place("/work/rocket/.git/config"))], Access::EditsGuarded),
        ("Write", "settings", vec![Effect::WritePath(Place::at(settings))], Access::EditsGuarded),
        ("Bash", "ls -la", vec![Effect::exec("ls -la")], Access::Executes),
        ("Bash", "cargo build", vec![Effect::exec("cargo build")], Access::Executes),
        ("WebFetch", "https://docs.rs/x", vec![Effect::Net { host: Some("docs.rs".into()) }], Access::Executes),
        ("Agent", "explore", vec![Effect::Spawn(SpawnKind::Agent)], Access::Reads),
        ("CronCreate", "nightly", vec![Effect::Spawn(SpawnKind::Schedule)], Access::Executes),
        ("PushNotification", "done", vec![Effect::Notify], Access::Reads),
        ("mcp__jira__search", "bug", vec![Effect::Remote { server: "jira".into() }], Access::Executes),
        ("Deploy", "prod", vec![Effect::Declared(Declared::Executes)], Access::Executes),
        ("Format", "a.rs", vec![Effect::Declared(Declared::Edits)], Access::Edits),
        ("TaskList", "", vec![], Access::Reads),
    ]
}

#[test]
fn every_tool_in_every_mode_gets_the_mode_table_s_verdict() {
    let none = Remembered::default();
    let bounds = bounds();
    for (tool, subject, effects, access) in cases() {
        for mode in PermissionMode::ALL {
            let call = Call { tool, subject, effects: &effects };
            let verdict = PolicyEngine::decide(&call, &context(mode, &none, &bounds));
            assert_eq!(outcome(&verdict), expected(mode, access), "{tool} {effects:?} {mode:?}: {verdict:?}");
        }
    }
}

#[test]
fn a_command_runs_without_asking_in_bypass_and_asks_once_remembered_otherwise() {
    let bounds = bounds();
    let effects = [Effect::exec("cargo test")];
    let call = Call { tool: "Bash", subject: "cargo test", effects: &effects };
    let mut remembered = Remembered::default();
    assert_eq!(PolicyEngine::decide(&call, &context(PermissionMode::Bypass, &remembered, &bounds)), Verdict::Allow);
    let judged = PolicyEngine::judge(&call, &context(PermissionMode::Ask, &remembered, &bounds));
    assert_eq!(judged.verdict, Verdict::Ask { reason: Reason::Mode, rememberable: true });
    remembered.remember("Bash", judged.standing, &judged.subject);
    assert_eq!(PolicyEngine::decide(&call, &context(PermissionMode::Ask, &remembered, &bounds)), Verdict::Allow);
}

#[test]
fn a_guarded_write_names_its_guard_and_unattended_asks_are_denied() {
    let bounds = bounds();
    let none = Remembered::default();
    let effects = [Effect::WritePath(place("/work/rocket/.git/hooks/pre-commit"))];
    let call = Call { tool: "Edit", subject: "pre-commit", effects: &effects };
    let judged = PolicyEngine::judge(&call, &context(PermissionMode::AcceptEdits, &none, &bounds));
    assert!(matches!(judged.verdict, Verdict::Ask { reason: Reason::Guarded { guard: Guard::GitFolder, .. }, .. }));
    let unattended = PolicyContext { no_prompts: true, ..context(PermissionMode::Ask, &none, &bounds) };
    assert!(unattended.settled(judged.verdict, "Edit").denies());
}

#[test]
fn the_ceiling_caps_the_mode() {
    let bounds = bounds();
    let none = Remembered::default();
    let effects = [Effect::exec("make")];
    let call = Call { tool: "Bash", subject: "make", effects: &effects };
    let capped =
        PolicyContext { ceiling: Some(PermissionMode::Plan), ..context(PermissionMode::Bypass, &none, &bounds) };
    assert!(PolicyEngine::decide(&call, &capped).denies());
}
