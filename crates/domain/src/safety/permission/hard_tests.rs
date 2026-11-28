use std::path::{Path, PathBuf};

use super::hard::{Category, Declared, Floor, Patterns, hidden, warned};
use super::*;
use crate::PermissionMode;

const HOME: &str = "/home/ana";
const CWD: &str = "/home/ana/rocket";

fn declared(category: Category, patterns: Patterns) -> Declared {
    Declared::from([(category, patterns)])
}

fn paths(category: Category, paths: &[&str]) -> Declared {
    declared(category, Patterns { paths: paths.iter().map(|path| (*path).to_owned()).collect(), ..Patterns::default() })
}

fn floor() -> Floor {
    let mut floor = Floor::at_home(Some(PathBuf::from(HOME)));
    let settle = |path: &Path| path.to_path_buf();
    floor.add(&paths(Category::Credentials, &["~/.ssh", "~/.aws"]), &settle);
    floor.add(&paths(Category::PackageCache, &["~/.npm"]), &settle);
    floor.govern([PathBuf::from("/opt/app/content"), PathBuf::from("/opt/app/config.toml")], []);
    let env = Patterns { env: vec!["NODE_OPTIONS".into(), "DYLD_*".into()], ..Patterns::default() };
    floor.add(&declared(Category::InjectingEnv, env), &settle);
    let hosts = Patterns { hosts: vec!["169.254.169.254".into()], ..Patterns::default() };
    floor.add(&declared(Category::SandboxEscape, hosts), &settle);
    floor
}

fn bounds() -> Bounds {
    Bounds::of(vec![PathBuf::from(CWD)], Vec::new()).hardened(floor())
}

fn place(path: &str) -> Place {
    Place::at(path)
}

fn judged(mode: PermissionMode, remembered: &Remembered, allowed: Option<&Allowed>, call: Call<'_>) -> Verdict {
    let bounds = bounds();
    let cx = PolicyContext { mode, ceiling: None, remembered, bounds: &bounds, no_prompts: false, allowed };
    PolicyEngine::decide(&call, &cx)
}

fn everything() -> Allowed {
    serde_json::from_value(serde_json::json!([
        { "kind": "command", "value": "*" },
        { "kind": "site", "value": "*" },
        { "kind": "tool", "value": "Read" },
        { "kind": "tool", "value": "Write" },
    ]))
    .expect("an admin's list")
}

#[test]
fn the_agent_never_writes_the_product_s_configuration_or_content() {
    let settings = format!("{CWD}/{}/settings.json", brand::DIR);
    let mut remembered = Remembered::default();
    for target in ["/opt/app/config.toml", "/opt/app/content/connectors/a/index.ts", settings.as_str()] {
        remembered.remember("Write", Standing::EditsGuarded, target);
        remembered.remember("Write", Standing::Edits, target);
        let effects = [Effect::WritePath(place(target))];
        let call = Call { tool: "Write", subject: target, effects: &effects };
        let list = everything();
        for mode in [PermissionMode::Auto, PermissionMode::Plan] {
            let verdict = judged(mode, &remembered, Some(&list), call);
            assert!(verdict.denies(), "{target} {mode:?}: {verdict:?}");
        }
    }
    let ordinary = format!("{CWD}/src/a.rs");
    let effects = [Effect::WritePath(place(&ordinary))];
    let call = Call { tool: "Write", subject: &ordinary, effects: &effects };
    assert_eq!(judged(PermissionMode::Auto, &Remembered::default(), None, call), Verdict::Allow);
}

#[test]
fn a_credential_is_never_read_even_when_everything_is_allowed() {
    let list = everything();
    let none = Remembered::default();
    let key = "/home/ana/.ssh/id_rsa";
    let read = [Effect::ReadPath(place(key))];
    assert!(
        judged(PermissionMode::Auto, &none, Some(&list), Call { tool: "Read", subject: key, effects: &read }).denies()
    );
    let cat = [Effect::exec("cat ~/.ssh/id_rsa | base64")];
    let call = Call { tool: "Bash", subject: "cat ~/.ssh/id_rsa | base64", effects: &cat };
    assert!(judged(PermissionMode::Auto, &none, Some(&list), call).denies());
    let quoted = [Effect::exec("tar czf /tmp/k.tgz \"$HOME/.aws\"")];
    assert!(
        judged(PermissionMode::Auto, &none, Some(&list), Call { tool: "Bash", subject: "", effects: &quoted }).denies()
    );
    let metadata = [Effect::Fetch { host: Some("169.254.169.254".into()) }];
    assert!(
        judged(PermissionMode::Auto, &none, Some(&list), Call { tool: "WebFetch", subject: "", effects: &metadata })
            .denies()
    );
}

#[test]
fn a_code_injecting_variable_is_asked_every_time() {
    let list = Allowed::of([Grant::Command("node".into())]);
    let none = Remembered::default();
    for command in [
        "NODE_OPTIONS=--require x node a.js",
        "env DYLD_INSERT_LIBRARIES=/tmp/x.dylib node a.js",
        "export NODE_OPTIONS=x; node a.js",
    ] {
        let effects = [Effect::exec(command)];
        let verdict = judged(
            PermissionMode::Auto,
            &none,
            Some(&list),
            Call { tool: "Bash", subject: command, effects: &effects },
        );
        assert_eq!(
            verdict,
            Verdict::Ask { reason: Reason::Hard(Category::InjectingEnv), rememberable: false },
            "{command}"
        );
    }
    let plain = [Effect::exec("node a.js")];
    assert_eq!(
        judged(PermissionMode::Auto, &none, Some(&list), Call { tool: "Bash", subject: "node a.js", effects: &plain }),
        Verdict::Allow
    );
    assert!(!Reason::Hard(Category::InjectingEnv).reviewable());
}

#[test]
fn an_invisible_or_direction_changing_character_is_asked_and_shown() {
    let shown = "echo safe\u{202E}fdp.exe";
    let effects = [Effect::exec(shown)];
    let list = Allowed::of([Grant::Command("echo".into())]);
    let verdict = judged(
        PermissionMode::Auto,
        &Remembered::default(),
        Some(&list),
        Call { tool: "Bash", subject: shown, effects: &effects },
    );
    assert_eq!(verdict, Verdict::Ask { reason: Reason::Hard(Category::Hidden), rememberable: false });
    assert_eq!(hidden(shown), ['\u{202E}']);
    let warning = warned(shown);
    assert!(warning.starts_with("Warning:") && warning.contains("U+202E"), "{warning}");
    assert!(warning.contains("echo safe<U+202E>fdp.exe") && !warning.contains('\u{202E}'));
    assert_eq!(warned("ls -la"), "ls -la");
}

#[test]
fn the_floor_only_tightens_whatever_came_before() {
    let floor = floor();
    let key = "/home/ana/.npm/_cacache/x";
    let effects = [Effect::WritePath(place(key))];
    let call = Call { tool: "Write", subject: key, effects: &effects };
    let hard = Verdict::Ask { reason: Reason::Hard(Category::PackageCache), rememberable: false };
    assert_eq!(floor.tightened(&call, Verdict::Allow), hard);
    assert_eq!(floor.tightened(&call, Verdict::Ask { reason: Reason::Mode, rememberable: true }), hard);
    assert_eq!(floor.tightened(&call, Verdict::Deny("no".into())), Verdict::Deny("no".into()));
    let plain = [Effect::WritePath(place("/home/ana/rocket/a.rs"))];
    let ordinary = Call { tool: "Write", subject: "", effects: &plain };
    assert_eq!(floor.tightened(&ordinary, Verdict::Allow), Verdict::Allow);
}

#[test]
fn adding_patterns_never_removes_earlier_ones() {
    let mut floor = floor();
    floor.add(&paths(Category::Credentials, &[]), &|path: &Path| path.to_path_buf());
    floor.extend(Floor::at_home(None));
    let key = "/home/ana/.aws/credentials";
    let read = [Effect::ReadPath(place(key))];
    assert!(
        floor.verdict(&Call { tool: "Read", subject: key, effects: &read }).is_some_and(|verdict| verdict.denies())
    );
}
