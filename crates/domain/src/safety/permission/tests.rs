use std::path::PathBuf;

use super::*;
use crate::PermissionMode;

const CWD: &str = "/work/rocket";

fn bounds() -> Bounds {
    Bounds::of(vec![PathBuf::from(CWD)], vec![PathBuf::from("/data/results")])
}

fn context<'a>(mode: PermissionMode, remembered: &'a Remembered, bounds: &'a Bounds) -> PolicyContext<'a> {
    PolicyContext { mode, ceiling: None, remembered, bounds, no_prompts: false, allowed: None }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Access {
    Reads,
    ReadsOutside,
    Edits,
    EditsGuarded,
    Executes,
    Commands,
    Fetches,
    Browses,
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
        (PermissionMode::Plan, Access::Commands | Access::Fetches) => Outcome::Allow,
        (PermissionMode::Plan, Access::Browses) => Outcome::Ask,
        (PermissionMode::Plan, _) => Outcome::Deny,
        (PermissionMode::AcceptEdits, Access::Edits) => Outcome::Allow,
        (_, Access::Commands | Access::Fetches | Access::Browses) => expected(mode, Access::Executes),
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
        ("Bash", "ls -la", vec![Effect::exec("ls -la")], Access::Commands),
        ("Bash", "cargo build", vec![Effect::exec("cargo build")], Access::Commands),
        ("WebFetch", "https://docs.rs/x", vec![Effect::Fetch { host: Some("docs.rs".into()) }], Access::Fetches),
        ("WebSearch", "rust", vec![Effect::Fetch { host: None }], Access::Fetches),
        ("Upload", "https://x.dev", vec![Effect::Net { host: Some("x.dev".into()) }], Access::Executes),
        ("Fetch", "and run", vec![Effect::Fetch { host: None }, Effect::exec("sh")], Access::Executes),
        ("Fetch", "and write", vec![Effect::Fetch { host: None }, Effect::WritePath(place("/work/rocket/a"))], Access::Executes),
        ("Agent", "explore", vec![Effect::Spawn(SpawnKind::Agent)], Access::Reads),
        ("ScheduleWakeup", "later", vec![Effect::Spawn(SpawnKind::Schedule)], Access::Executes),
        ("mcp__jira__search", "bug", vec![Effect::Remote { server: "jira".into() }], Access::Executes),
        ("mcp__browser__navigate", "site", vec![Effect::Remote { server: crate::mcp::browser_server() }], Access::Browses),
        ("mcp__browser__mixed", "site", vec![Effect::Remote { server: crate::mcp::browser_server() }, Effect::exec("sh")], Access::Executes),
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
    let effects = [Effect::WritePath(place("/work/rocket/src/a.rs"))];
    let call = Call { tool: "Write", subject: "src/a.rs", effects: &effects };
    let capped =
        PolicyContext { ceiling: Some(PermissionMode::Plan), ..context(PermissionMode::Bypass, &none, &bounds) };
    assert!(PolicyEngine::decide(&call, &capped).denies());
}

#[test]
fn with_the_list_every_mode_asks_for_what_is_not_on_it_and_lets_through_what_is() {
    let bounds = bounds();
    let none = Remembered::default();
    let list = Allowed::of([Grant::Command("cargo test".into()), Grant::Site("gov.br".into()), Grant::Tool("mcp__jira__*".into())]);
    let browser = crate::mcp::browser_server();
    let judged = |tool: &str, effects: Vec<Effect>, mode| {
        let call = Call { tool, subject: "", effects: &effects };
        let cx = PolicyContext { allowed: Some(&list), ..context(mode, &none, &bounds) };
        outcome(&PolicyEngine::decide(&call, &cx))
    };
    for mode in [PermissionMode::Ask, PermissionMode::AcceptEdits, PermissionMode::Bypass, PermissionMode::Plan] {
        assert_eq!(judged("Bash", vec![Effect::exec("cargo test -p x && cargo test")], mode), Outcome::Allow, "{mode:?}");
        assert_eq!(judged("Bash", vec![Effect::exec("cargo test; rm -rf /")], mode), Outcome::Ask, "{mode:?}");
        assert_eq!(judged("Bash", vec![Effect::exec("cargo build")], mode), Outcome::Ask, "{mode:?}");
        let page = |host: &str| vec![Effect::Remote { server: browser.clone() }, Effect::Fetch { host: Some(host.into()) }];
        assert_eq!(judged("mcp__b__read_page", page("www.gov.br"), mode), Outcome::Allow, "{mode:?}");
        assert_eq!(judged("mcp__b__read_page", page("evil.dev"), mode), Outcome::Ask, "{mode:?}");
        let unknown = vec![Effect::Remote { server: browser.clone() }, Effect::Fetch { host: None }];
        assert_eq!(judged("mcp__b__computer", unknown, mode), Outcome::Ask, "{mode:?}");
        assert_eq!(judged("mcp__b__tabs_context_mcp", vec![Effect::Remote { server: browser.clone() }], mode), Outcome::Allow);
    }
    let jira = || vec![Effect::Remote { server: "jira".into() }];
    assert_eq!(judged("mcp__jira__search", jira(), PermissionMode::Bypass), Outcome::Allow);
    assert_eq!(judged("mcp__github__merge", vec![Effect::Remote { server: "github".into() }], PermissionMode::Bypass), Outcome::Ask);
    assert_eq!(judged("mcp__jira__search", jira(), PermissionMode::Plan), Outcome::Deny);
    let edit = vec![Effect::WritePath(place("/work/rocket/src/a.rs"))];
    assert_eq!(judged("Write", edit, PermissionMode::Bypass), Outcome::Allow);
}

#[test]
fn always_allowing_learns_the_command_prefix_the_site_or_the_tool() {
    let learned = |tool: &str, effects: Vec<Effect>| Allowed::learned(&Call { tool, subject: "", effects: &effects });
    assert_eq!(
        learned("Bash", vec![Effect::exec("cargo test --workspace && git status -s")]),
        [Grant::Command("cargo test".into()), Grant::Command("git status".into())]
    );
    assert_eq!(learned("Bash", vec![Effect::exec("ls -la src")]), [Grant::Command("ls".into())]);
    let browser = crate::mcp::browser_server();
    let page = vec![Effect::Remote { server: browser }, Effect::Fetch { host: Some("www.gov.br".into()) }];
    assert_eq!(learned("mcp__b__navigate", page), [Grant::Site("www.gov.br".into())]);
    assert_eq!(learned("mcp__jira__search", vec![Effect::Remote { server: "jira".into() }]), [Grant::Tool("mcp__jira__search".into())]);
    let mut list = Allowed::default();
    assert!(list.add(Grant::Site("https://Docs.rs/serde".into())));
    assert!(!list.add(Grant::Site("docs.rs".into())));
    assert!(list.site_allowed("api.docs.rs") && !list.site_allowed("notdocs.rs"));
    assert!(list.remove(&Grant::Site("docs.rs".into())));
}
