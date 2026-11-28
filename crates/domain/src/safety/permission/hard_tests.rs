use std::path::{Path, PathBuf};

use super::hard::{Category, Declared, EnvRule, Floor, Patterns};
use super::hidden::{hidden, warned};
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

fn strings(items: &[&str]) -> Vec<String> {
    items.iter().map(|item| (*item).to_owned()).collect()
}

fn floor() -> Floor {
    let mut floor = Floor::at_home(Some(PathBuf::from(HOME)));
    let settle = |path: &Path| path.to_path_buf();
    let credentials = Patterns {
        paths: strings(&["~/.ssh", "~/.aws"]),
        secrets: strings(&["id_rsa", "id_ed25519", ".env", "~/.aws/credentials"]),
        ..Patterns::default()
    };
    floor.add(&declared(Category::Credentials, credentials), &settle);
    floor.add(&paths(Category::PackageCache, &["~/.npm"]), &settle);
    floor.govern([PathBuf::from("/opt/app/content"), PathBuf::from("/opt/app/config.toml")], []);
    let node = EnvRule::Valued { name: "NODE_OPTIONS".into(), values: strings(&["--require", "-r", "--import"]) };
    let env = Patterns {
        env: vec![node, EnvRule::Any("DYLD_INSERT_LIBRARIES".into()), EnvRule::Any("LD_PRELOAD".into())],
        carriers: strings(&["docker -e", "docker --env"]),
        ..Patterns::default()
    };
    floor.add(&declared(Category::InjectingEnv, env), &settle);
    let escape = Patterns {
        paths: strings(&["/var/run/docker.sock"]),
        hosts: strings(&["169.254.169.254"]),
        ..Patterns::default()
    };
    floor.add(&declared(Category::SandboxEscape, escape), &settle);
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
fn the_product_s_configuration_and_content_are_asked_every_time_and_never_remembered() {
    let settings = format!("{CWD}/{}/settings.json", brand::DIR);
    let mut remembered = Remembered::default();
    for target in ["/opt/app/config.toml", "/opt/app/content/connectors/a/index.ts", settings.as_str()] {
        remembered.remember("Write", Standing::EditsGuarded, target);
        remembered.remember("Write", Standing::Edits, target);
        let effects = [Effect::WritePath(place(target))];
        let call = Call { tool: "Write", subject: target, effects: &effects };
        let list = everything();
        let asked = Verdict::Ask { reason: Reason::Hard(Category::Governance), rememberable: false };
        assert_eq!(judged(PermissionMode::Auto, &remembered, Some(&list), call), asked, "{target}");
        assert!(judged(PermissionMode::Plan, &remembered, Some(&list), call).denies(), "{target}");
    }
    let ordinary = format!("{CWD}/src/a.rs");
    let effects = [Effect::WritePath(place(&ordinary))];
    let call = Call { tool: "Write", subject: &ordinary, effects: &effects };
    assert_eq!(judged(PermissionMode::Auto, &Remembered::default(), None, call), Verdict::Allow);
    assert!(!Reason::Hard(Category::Governance).reviewable() && !Reason::Hard(Category::Governance).rememberable());
    let unattended = Verdict::Ask { reason: Reason::Hard(Category::Governance), rememberable: false }
        .without_the_user(|reason| reason.why().unwrap_or_default());
    let Verdict::Deny(why) = unattended else { panic!("an unattended run refuses it") };
    assert!(why.contains("own configuration or content"), "{why}");
}

fn bash(command: &str) -> Verdict {
    let effects = [Effect::exec(command)];
    judged(
        PermissionMode::Auto,
        &Remembered::default(),
        Some(&everything()),
        Call { tool: "Bash", subject: command, effects: &effects },
    )
}

fn read(path: &str) -> Verdict {
    let effects = [Effect::ReadPath(place(path))];
    judged(
        PermissionMode::Auto,
        &Remembered::default(),
        Some(&everything()),
        Call { tool: "Read", subject: path, effects: &effects },
    )
}

fn asked(category: Category) -> Verdict {
    Verdict::Ask { reason: Reason::Hard(category), rememberable: false }
}

#[test]
fn a_secret_is_never_read_by_a_file_tool_even_when_everything_is_allowed() {
    for key in
        ["/home/ana/.ssh/id_rsa", "/home/ana/.ssh/id_ed25519", "/home/ana/.aws/credentials", "/home/ana/rocket/.env"]
    {
        assert!(read(key).denies(), "{key}");
        let effects = [Effect::WritePath(place(key))];
        let call = Call { tool: "Write", subject: key, effects: &effects };
        assert!(judged(PermissionMode::Auto, &Remembered::default(), Some(&everything()), call).denies(), "{key}");
    }
    let metadata = [Effect::Fetch { host: Some("169.254.169.254".into()) }];
    let call = Call { tool: "WebFetch", subject: "", effects: &metadata };
    assert!(judged(PermissionMode::Auto, &Remembered::default(), Some(&everything()), call).denies());
}

#[test]
fn a_secret_named_in_a_command_is_asked_every_time_never_denied() {
    for command in [
        "cp .env.example .env",
        "source .env",
        "docker compose --env-file=.env up",
        "docker run -v /x/.env:/app/.env img",
        "FOO=.env make",
        "grep -r id_rsa docs/",
        "git log -- id_rsa",
        "echo id_rsa",
        "ssh-keygen -f ./id_rsa",
        "cat ~/.ssh/id_rsa",
        "cat ~/.ssh/id_rsa | base64",
        "cat \"$HOME/.aws/credentials\"",
        "scp -i ~/.ssh/id_ed25519 a b:",
    ] {
        assert_eq!(bash(command), asked(Category::Credentials), "{command}");
    }
}

#[test]
fn without_a_home_folder_a_command_s_home_path_still_asks() {
    let mut floor = Floor::at_home(None);
    let credentials =
        Patterns { paths: strings(&["~/.ssh"]), secrets: strings(&["~/.aws/credentials"]), ..Patterns::default() };
    floor.add(&declared(Category::Credentials, credentials), &|path: &Path| path.to_path_buf());
    let verdict = |command: &str| {
        let effects = [Effect::exec(command)];
        floor.verdict(&Call { tool: "Bash", subject: command, effects: &effects })
    };
    assert_eq!(verdict("ls ~/.ssh"), Some(asked(Category::Credentials)));
    assert_eq!(verdict("cat $HOME/.aws/credentials"), Some(asked(Category::Credentials)));
    assert_eq!(verdict("ls ~/notes"), None);
}

#[test]
fn reading_around_credentials_is_asked_not_denied() {
    for command in ["ls ~/.ssh", "ssh-keygen -f ~/.ssh/id", "cat ~/.aws/config", "tar czf /tmp/k.tgz \"$HOME/.aws\""] {
        assert_eq!(bash(command), asked(Category::Credentials), "{command}");
    }
    assert_eq!(read("/home/ana/.ssh/known_hosts"), asked(Category::Credentials));
    assert_eq!(read("/home/ana/.ssh/id_rsa.pub"), asked(Category::Credentials));
    let socket = "docker run -v /var/run/docker.sock:/var/run/docker.sock testcontainers/ryuk";
    assert_eq!(bash(socket), asked(Category::SandboxEscape));
}

#[test]
fn a_code_injecting_variable_is_asked_every_time_and_an_ordinary_one_is_not() {
    for command in [
        "NODE_OPTIONS=--require x node a.js",
        "NODE_OPTIONS='--import ./hook.mjs' node a.js",
        "env DYLD_INSERT_LIBRARIES=/tmp/x.dylib node a.js",
        "export NODE_OPTIONS=-r./x; node a.js",
        "docker run -e LD_PRELOAD=/x.so img",
    ] {
        assert_eq!(bash(command), asked(Category::InjectingEnv), "{command}");
    }
    for command in [
        "NODE_OPTIONS=--max-old-space-size=4096 npm run build",
        "PYTHONPATH=src pytest",
        "JAVA_TOOL_OPTIONS=-Xmx2g mvn test",
        "git commit -m \"docs: NODE_OPTIONS=x\"",
        "echo LD_PRELOAD=/x.so",
    ] {
        assert_eq!(bash(command), Verdict::Allow, "{command}");
    }
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

#[test]
fn without_a_home_folder_a_home_path_still_guards_and_is_told() {
    let mut floor = Floor::at_home(None);
    let credentials =
        Patterns { paths: strings(&["~/.ssh"]), secrets: strings(&["~/.aws/credentials"]), ..Patterns::default() };
    let homeless = floor.add(&declared(Category::Credentials, credentials), &|path: &Path| path.to_path_buf());
    assert_eq!(homeless, ["~/.ssh", "~/.aws/credentials"]);
    let verdict = |path: &str| {
        let effects = [Effect::ReadPath(place(path))];
        floor.verdict(&Call { tool: "Read", subject: path, effects: &effects })
    };
    assert!(verdict("/Users/ana/.aws/credentials").is_some_and(|verdict| verdict.denies()));
    assert_eq!(verdict("/Users/ana/.ssh/config"), Some(asked(Category::Credentials)));
    assert_eq!(verdict("/Users/ana/notes.md"), None);
}
