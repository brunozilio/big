#![cfg(unix)]

use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use domain::events::BackgroundChange;
use domain::output::Shaping;
use serde_json::json;

use super::shipped;
use crate::tools::builtin::shell::{savings, shells};
use crate::tools::{Effect, Outcome, SpawnKind, Tool, Workspace, inbox};

fn of_client(client: u64) -> Workspace {
    let mut workspace = Workspace::new(std::env::temp_dir());
    workspace.credentials.client = client;
    workspace
}

fn bash(input: serde_json::Value, workspace: &Workspace) -> Outcome {
    shipped("Bash").run(&input, workspace)
}

fn read(id: &str, client: u64) -> Outcome {
    shipped("BashOutput").run(&json!({ "bash_id": id }), &of_client(client))
}

fn started(client: u64, script: &str) -> String {
    let mut command = std::process::Command::new("/bin/sh");
    command.arg("-c").arg(script).stdin(std::process::Stdio::null());
    shells::start(&of_client(client), script, &mut command).expect("start")
}

fn settled(client: u64) {
    let deadline = Instant::now() + Duration::from_secs(120);
    while !shells::settled(client) {
        assert!(Instant::now() < deadline, "the command never ended");
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn the_shell_tools_declare_what_they_do_and_describe_themselves() {
    let workspace = Workspace::new(std::env::temp_dir());
    let tool = shipped("Bash");
    assert_eq!(tool.effects(&json!({ "command": "ls" }), &workspace), [Effect::exec("ls")]);
    assert_eq!(tool.effects(&serde_json::Value::Null, &workspace), [Effect::exec("")]);
    assert_eq!(tool.summary(&json!({ "command": "ls -la" })), "ls -la");
    let described = &tool.spec.description;
    assert!(described.contains("(default 120000, at most 600000)"), "{described}");
    assert!(described.contains("Output past 30000 characters"), "{described}");
    let named = format!("This computer runs commands with {}.", platform::shell_name());
    assert!(described.contains(&named), "{described}");
    let describe = |shell: &str| {
        let data = json!({ "shell": shell });
        crate::tools::contract::quick(&tool.shown_path(), &tool.source, "spec.description", &[], &data)
            .ok()
            .and_then(|text| text.as_str().map(str::to_owned))
            .unwrap_or_default()
    };
    assert!(describe("zsh").contains("This computer runs commands with zsh. In zsh, `status` is a read-only variable"));
    assert!(!describe("bash").contains("read-only"));
    let monitor = shipped("Monitor");
    let effects = monitor.effects(&json!({ "command": "tail -f x" }), &workspace);
    assert_eq!(effects, [Effect::exec("tail -f x"), Effect::Spawn(SpawnKind::BackgroundShell)]);
    assert_eq!(monitor.summary(&json!({ "command": "tail -f x", "description": "the log" })), "the log");
    assert_eq!(monitor.summary(&json!({ "command": "tail -f x" })), "tail -f x");
    let output = shipped("BashOutput");
    assert_eq!(output.effects(&json!({ "bash_id": "bash_1" }), &workspace), []);
    assert_eq!(output.summary(&json!({ "bash_id": "bash_1" })), "bash_1");
}

#[test]
fn runs_in_the_working_directory_and_reports_failures() {
    let dir = tempfile::tempdir().expect("dir");
    std::fs::write(dir.path().join("here.txt"), "").expect("write");
    let workspace = Workspace::new(dir.path());
    assert_eq!(bash(json!({"command": "ls"}), &workspace), Outcome::ok("here.txt"));

    let failed = bash(json!({"command": "echo out; echo err >&2; exit 3"}), &workspace);
    assert_eq!(failed, Outcome::error("out\nerr\n(exit code 3)"));
    assert_eq!(bash(json!({"command": "true"}), &workspace).content, "(no output)");
    assert_eq!(bash(json!({}), &workspace), Outcome::error("Missing required parameter `command`."));
}

#[test]
fn with_output_cleanup_a_command_runs_without_colours_and_its_escape_codes_are_dropped() {
    let mut workspace = Workspace::new(std::env::temp_dir());
    let command = json!({"command": "printf '\\033[31m%s\\033[0m' \"$NO_COLOR\""});
    assert!(bash(command.clone(), &workspace).content.starts_with("\u{1b}[31m"));

    workspace.policy.output.cleanup = true;
    assert_eq!(bash(command, &workspace), Outcome::ok("1"));
}

#[test]
fn a_long_output_is_cut_to_the_limit_with_a_first_line_saying_how_much_and_where_the_whole_is() {
    let mut workspace = Workspace::new(std::env::temp_dir());
    workspace.session_id = format!("bash-limit-{}", std::process::id());
    workspace.policy.bash_output = domain::output::OutputLimit::of(1_000);
    let outcome = bash(json!({"command": "seq 1 2000"}), &workspace);
    assert!(!outcome.is_error, "{}", outcome.content);
    let (note, rest) = outcome.content.split_once('\n').expect("note");
    assert!(note.starts_with("[Output cut: 8892 characters in 2000 lines, over the limit of 1000"), "{note}");
    let path = note
        .split_once("The full output is in ")
        .and_then(|(_, after)| after.strip_suffix(": Read or Grep it for the rest.]"))
        .expect("the path");
    let whole = std::fs::read_to_string(path).expect("saved");
    let _ = std::fs::remove_dir_all(crate::tools::persisted::results_dir(&workspace.session_id));
    assert_eq!(whole.lines().count(), 2000);
    assert!(rest.starts_with("1\n2\n3\n") && rest.ends_with("\n1999\n2000"), "{rest}");
    assert!(rest.chars().count() < 1_100);
    assert!(rest.contains("\n… [7892 characters cut here] …\n"), "{rest}");

    let failed = bash(json!({"command": "seq 1 2000; exit 4"}), &workspace);
    assert!(failed.is_error && failed.content.starts_with("[Output cut: "), "{}", failed.content);
    assert!(failed.content.ends_with("\n2000\n(exit code 4)"), "{}", failed.content);
    let _ = std::fs::remove_dir_all(crate::tools::persisted::results_dir(&workspace.session_id));
    assert_eq!(bash(json!({"command": "seq 1 3"}), &workspace).content, "1\n2\n3");
}

#[test]
fn stops_at_the_timeout() {
    let workspace = Workspace::new(std::env::temp_dir());
    let started = Instant::now();
    let outcome = bash(json!({"command": "sleep 60", "timeout": 200}), &workspace);
    assert!(outcome.is_error && outcome.content.contains("stopped after 200 ms"), "{outcome:?}");
    assert!(started.elapsed() < Duration::from_secs(30), "the command ran on: {:?}", started.elapsed());
}

#[test]
fn a_command_started_in_the_background_says_so_beside_its_text() {
    let workspace = of_client(93_021);
    let started = bash(json!({"command": "sleep 30", "run_in_background": true}), &workspace);
    let Some(BackgroundChange::Started(id)) = started.background.as_deref().cloned() else {
        panic!("no background start in {started:?}")
    };
    assert!(started.content.contains(&id), "{started:?}");
    assert!(shells::kill(93_021, &id));
    assert_eq!(bash(json!({"command": "true"}), &workspace).background, None);
}

fn fake_cargo(dir: &Path) {
    let passing: String = (0..40).map(|i| format!("echo 'test case_{i} ... ok'\n")).collect();
    let script = format!(
        "#!/bin/sh\nprintf '\\033[32m   Compiling\\033[0m x v0.1.0\\n'\n{passing}echo 'test broken ... FAILED'\nprintf 'test result: FAILED'\n"
    );
    std::fs::write(dir.join("cargo"), script).expect("cargo");
    std::fs::set_permissions(dir.join("cargo"), std::os::unix::fs::PermissionsExt::from_mode(0o755)).expect("mode");
}

fn shaped_workspace(dir: &Path, output: Shaping, client: u64) -> Workspace {
    fake_cargo(dir);
    let mut workspace = Workspace::new(dir);
    workspace.session_id = format!("shaped-{client}");
    workspace.credentials.client = client;
    workspace.policy.output = output;
    workspace
}

#[test]
fn with_every_flag_off_the_output_is_left_as_it_came() {
    let dir = tempfile::tempdir().expect("dir");
    let workspace = shaped_workspace(dir.path(), Shaping::default(), 93_001);
    let shown = bash(json!({ "command": "./cargo test" }), &workspace).content;
    assert!(shown.starts_with("\u{1b}[32m   Compiling\u{1b}[0m x v0.1.0\ntest case_0 ... ok"), "{shown}");
    assert!(shown.ends_with("test broken ... FAILED\ntest result: FAILED"), "{shown}");
    assert!(savings::list(93_001).is_empty());
}

#[test]
fn cleanup_alone_drops_escape_codes_but_keeps_every_line() {
    let dir = tempfile::tempdir().expect("dir");
    let workspace = shaped_workspace(dir.path(), Shaping { cleanup: true, ..Shaping::default() }, 93_002);
    let shown = bash(json!({ "command": "./cargo test" }), &workspace).content;
    assert!(shown.starts_with("   Compiling x v0.1.0\ntest case_0 ... ok"), "{shown}");
    assert!(savings::list(93_002).is_empty());
}

#[test]
fn filters_hide_the_noise_keep_the_whole_output_and_count_the_saving() {
    let dir = tempfile::tempdir().expect("dir");
    let workspace = shaped_workspace(dir.path(), Shaping { cleanup: true, filters: true, savings: true }, 93_003);
    let shown = bash(json!({ "command": "./cargo test" }), &workspace).content;
    let (kept, footer) = shown.split_once("\n[").expect("footer");
    assert_eq!(kept, "test broken ... FAILED\ntest result: FAILED");
    assert!(footer.starts_with("41 lines of cargo noise hidden (passing tests, progress, build steps"), "{footer}");
    let whole = footer.rsplit_once("the whole output is in ").and_then(|(_, path)| path.strip_suffix(']'));
    let saved = std::fs::read_to_string(whole.expect("path")).expect("saved");
    let _ = std::fs::remove_dir_all(crate::tools::persisted::results_dir(&workspace.session_id));
    assert!(saved.contains("test case_39 ... ok") && !saved.contains('\u{1b}'), "{saved}");

    let recorded = savings::list(93_003);
    assert_eq!(recorded.len(), 1);
    assert_eq!(recorded[0].program, "cargo");
    assert!(recorded[0].after < recorded[0].before / 2, "{recorded:?}");
    savings::forget(93_003);
}

fn git(dir: &Path, args: &[&str]) {
    let identity = ["-c", "user.name=t", "-c", "user.email=t@example.test", "-c", "commit.gpgsign=false"];
    let done = platform::git::command(dir).args(identity).args(args).output().expect("git");
    assert!(done.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&done.stderr));
}

#[test]
fn a_change_rewind_cannot_undo_is_named_after_the_output_and_the_rest_are_recorded() {
    let dir = tempfile::tempdir().expect("dir");
    git(dir.path(), &["init", "-q"]);
    git(dir.path(), &["config", "filter.crypt.smudge", "tr a-z n-za-m"]);
    git(dir.path(), &["config", "filter.crypt.clean", "tr a-z n-za-m"]);
    std::fs::write(dir.path().join(".gitattributes"), "secret.txt filter=crypt\n").expect("attributes");
    std::fs::write(dir.path().join("secret.txt"), "plain\n").expect("secret");
    std::fs::write(dir.path().join("z.txt"), "one\n").expect("z.txt");
    let an_hour_ago = std::time::SystemTime::now() - Duration::from_secs(3600);
    let secret = std::fs::File::options().write(true).open(dir.path().join("secret.txt")).expect("secret");
    secret.set_modified(an_hour_ago).expect("an mtime git will not doubt");
    git(dir.path(), &["add", "."]);
    git(dir.path(), &["commit", "-qm", "first"]);
    let mut workspace = Workspace::new(dir.path());
    workspace.services.files = Some(Arc::default());

    let outcome = bash(json!({ "command": "echo two > z.txt; echo overwritten > secret.txt; echo done" }), &workspace);
    let secret = dir.path().canonicalize().expect("root").join("secret.txt");
    let note = format!(
        "done\n(/rewind cannot restore {}: the repository runs a filter on them that was not run here, so what git \
         stores is not what was checked out)",
        secret.display()
    );
    assert_eq!(outcome, Outcome::ok(note));
    let files = workspace.services.files.as_ref().expect("files").take().files;
    let names: Vec<String> =
        files.keys().map(|path| path.file_name().expect("name").to_string_lossy().into_owned()).collect();
    assert_eq!(names, ["z.txt"]);
}

#[test]
fn a_background_command_is_read_incrementally_and_stopped() {
    let id = started(93_101, "echo ready; sleep 30");
    let deadline = Instant::now() + Duration::from_secs(120);
    let mut first = read(&id, 93_101).content;
    while !first.contains("ready") {
        assert!(Instant::now() < deadline, "{first}");
        std::thread::sleep(Duration::from_millis(20));
        first = read(&id, 93_101).content;
    }
    assert!(first.contains("running for"), "{first}");
    let unread = read(&id, 93_101);
    assert!(unread.content.contains("(no new output)"), "output is read once");
    assert_eq!(unread.background, None, "a running command has not ended");
    assert!(read(&id, 93_102).content.contains("No background command"), "another client cannot see it");
    assert_eq!(shells::list(93_101), [(id.clone(), "echo ready; sleep 30".to_owned(), true)]);
    let stopped = super::shipped("KillShell").run(&json!({ "shell_id": id }), &of_client(93_101));
    assert!(stopped.content.starts_with("Stopped"), "{stopped:?}");
    assert_eq!(stopped.background.as_deref(), Some(&BackgroundChange::Ended(id.clone())));
    assert!(shells::list(93_101).is_empty());
}

#[test]
fn a_person_looks_at_the_last_output_without_taking_it_from_the_agent() {
    let id = started(93_107, "echo one; echo two; sleep 30");
    let deadline = Instant::now() + Duration::from_secs(120);
    while !shells::tail(93_107, &id, 100).is_some_and(|seen| seen.contains("two")) {
        assert!(Instant::now() < deadline, "the output never came");
        std::thread::sleep(Duration::from_millis(20));
    }
    assert_eq!(shells::tail(93_107, &id, 4).as_deref(), Some("two"), "only the last bytes, trimmed");
    assert_eq!(shells::tail(93_108, &id, 100), None, "another client cannot see it");
    assert!(read(&id, 93_107).content.contains("one\ntwo"), "the agent still reads it all");
    assert!(shells::kill(93_107, &id));
}

#[test]
fn a_finished_command_says_how_and_a_leaving_client_stops_its_own() {
    let done = started(93_103, "echo built; exit 2");
    let deadline = Instant::now() + Duration::from_secs(120);
    let mut text = read(&done, 93_103).content;
    while !text.contains("finished") {
        assert!(Instant::now() < deadline, "{text}");
        std::thread::sleep(Duration::from_millis(20));
        text.push_str(&read(&done, 93_103).content);
    }
    assert!(text.contains("exit code 2") && text.contains("built"), "{text}");
    started(93_104, "sleep 30");
    shells::kill_client(93_104);
    assert!(shells::list(93_104).is_empty());
}

#[test]
fn a_background_read_says_when_output_was_dropped_before_it_was_read() {
    let id = started(93_122, "head -c 3000000 /dev/zero | tr '\\0' 'a'; echo; echo the end");
    settled(93_122);
    let text = read(&id, 93_122).content;
    let mut lines = text.lines();
    assert!(lines.next().is_some_and(|state| state.contains("finished (exit code 0)")), "{text:.120}");
    let told = lines.next().unwrap_or_default();
    assert!(told.starts_with("[earlier output dropped: showing the last "), "{told:.120}");
    assert!(told.ends_with(&format!(" of {} bytes]", 3_000_000 + 1 + 8)), "{told:.120}");
    assert!(text.ends_with(" more characters cut]"), "the read is clipped");
}

#[test]
fn a_finished_command_leaves_the_list_once_its_last_output_was_read() {
    let id = started(93_123, "echo built");
    settled(93_123);
    assert_eq!(shells::list(93_123).len(), 1, "it stays until it is read");
    let read_once = read(&id, 93_123);
    assert!(read_once.content.contains("finished") && read_once.content.contains("built"), "{read_once:?}");
    assert_eq!(read_once.background.as_deref(), Some(&BackgroundChange::Ended(id.clone())));
    assert!(shells::list(93_123).is_empty(), "read to its end, it is forgotten");
    assert!(read(&id, 93_123).content.contains("No background command"));
}

#[test]
fn a_read_keeps_only_the_lines_its_filter_names() {
    let id = started(93_124, "echo keep one; echo drop; echo keep two");
    settled(93_124);
    let filtered = shipped("BashOutput").run(&json!({ "bash_id": id, "filter": "keep" }), &of_client(93_124));
    assert_eq!(
        filtered,
        Outcome::ok(format!("[{id}: finished (exit code 0)]\nkeep one\nkeep two"))
            .in_the_background(BackgroundChange::Ended(id.clone()))
    );
}

#[test]
fn each_line_a_monitor_prints_reaches_the_inbox_and_so_does_its_end() {
    let dir = tempfile::tempdir().expect("dir");
    let mut workspace = Workspace::new(dir.path());
    workspace.credentials.client = 93_201;
    let input = json!({"command": "echo ready; sleep 0.7; echo ERROR boom"});
    let started = shipped("Monitor").run(&input, &workspace);
    assert!(!started.is_error && started.content.contains("Monitoring as bash_"), "{started:?}");
    let deadline = Instant::now() + Duration::from_secs(120);
    let mut seen = Vec::new();
    while !seen.iter().any(|n: &domain::inbox::Notice| n.text.contains("finished")) {
        assert!(Instant::now() < deadline, "{seen:?}");
        std::thread::sleep(Duration::from_millis(100));
        seen.extend(inbox::take(93_201));
    }
    let texts: Vec<&str> = seen.iter().map(|n| n.text.as_str()).collect();
    assert!(texts.contains(&"ready"), "{texts:?}");
    assert!(texts.contains(&"ERROR boom"), "{texts:?}");
    assert!(seen.iter().all(|n| n.source.starts_with("monitor bash_")));
}

#[test]
fn reading_a_monitor_with_bash_output_loses_or_repeats_no_line() {
    let dir = tempfile::tempdir().expect("dir");
    let mut workspace = Workspace::new(dir.path());
    workspace.credentials.client = 93_202;
    let input = json!({"command": "echo one; sleep 1; echo two; sleep 1; echo three"});
    let started = shipped("Monitor").run(&input, &workspace);
    let id = started.content.split_whitespace().nth(2).expect("id").trim_end_matches(':').to_owned();
    let deadline = Instant::now() + Duration::from_secs(120);
    let mut lines = Vec::new();
    let mut was_read = false;
    while !lines.iter().any(|text: &String| text.contains("finished")) {
        assert!(Instant::now() < deadline, "{lines:?}");
        std::thread::sleep(Duration::from_millis(100));
        lines.extend(inbox::take(93_202).into_iter().map(|notice| notice.text));
        if !was_read && lines.iter().any(|text| text == "one") {
            read(&id, 93_202);
            was_read = true;
        }
    }
    let events: Vec<&str> = lines.iter().map(String::as_str).filter(|text| !text.contains("finished")).collect();
    assert_eq!(events, ["one", "two", "three"]);
}
