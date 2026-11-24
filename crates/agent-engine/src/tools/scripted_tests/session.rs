use std::path::Path;
use std::sync::Arc;

use serde_json::{Value, json};

use super::shipped;
use crate::lsp::fake::ScriptedServer;
use crate::model::ToolSpec;
use crate::tools::builtin::schedules;
use crate::tools::tool_search::{Deferred, defer, loaded_names};
use crate::tools::{Effect, Tool, Workspace, peers};

#[test]
fn a_wakeup_is_scheduled_once_clamped_to_its_bounds_and_asks_first() {
    let mut workspace = Workspace::new("/work/api");
    workspace.credentials.client = 7106;
    let wakeup = shipped("ScheduleWakeup");
    let acts = |input: &Value| wakeup.effects(input, &workspace).iter().any(Effect::acts);
    assert!(acts(&Value::Null), "a planted prompt would run unasked");
    assert_eq!(wakeup.effects(&Value::Null, &workspace), [Effect::Spawn(crate::tools::SpawnKind::Schedule)]);
    assert_eq!(wakeup.summary(&json!({ "reason": "the build takes a while" })), "the build takes a while");
    assert_eq!(wakeup.summary(&Value::Null), "");

    let soon = wakeup.run(&json!({ "delaySeconds": 5, "prompt": "check the build" }), &workspace);
    assert!(!soon.is_error, "{}", soon.content);
    assert!(soon.content.starts_with("Wakeup in 60s: wakeup_"), "{}", soon.content);
    assert!(soon.content.ends_with(": check the build"), "{}", soon.content);
    let late = wakeup.run(&json!({ "delaySeconds": 99999.7, "prompt": "later" }), &workspace);
    assert!(late.content.starts_with("Wakeup in 3600s: "), "{}", late.content);
    let listed = schedules::list(7106);
    assert_eq!(listed.len(), 2);
    let local = schedules::local_minute(listed[0].next);
    assert!(soon.content.contains(&format!(", at {local}: ")), "{} {local}", soon.content);

    let missing = wakeup.run(&json!({ "delaySeconds": 90 }), &workspace);
    assert_eq!((missing.content.as_str(), missing.is_error), ("Missing required parameter `prompt`.", true));
    let no_delay = wakeup.run(&json!({ "prompt": "x" }), &workspace);
    assert_eq!(no_delay.content, "Missing required parameter `delaySeconds`.");
    for _ in listed.len()..domain::schedule::MOST {
        schedules::add(7106, "filler", schedules::now() + 600).expect("room");
    }
    let full = wakeup.run(&json!({ "delaySeconds": 90, "prompt": "one more" }), &workspace);
    assert!(full.is_error, "{}", full.content);
    assert_eq!(full.content, format!("At most {} wakeups can be scheduled.", domain::schedule::MOST));
    schedules::forget_client(7106);
}

#[test]
fn list_sessions_names_this_session_and_the_others_with_their_folders() {
    let tool = shipped("ListSessions");
    assert!(tool.spec.description.contains(brand::NAME), "{}", tool.spec.description);
    assert!(!tool.spec.description.contains("{{"), "{}", tool.spec.description);
    let mut workspace = Workspace::new("/work/api");
    workspace.credentials.client = 9411;
    assert!(tool.effects(&Value::Null, &workspace).is_empty());
    assert_eq!(tool.summary(&json!({})), "");
    let alone = tool.run(&json!({}), &workspace);
    assert!(alone.content.starts_with("This session is "), "{}", alone.content);
    peers::register(9411, Path::new("/work/api"));
    peers::register(9412, Path::new("/work/web"));
    let listed = tool.run(&json!({}), &workspace);
    assert!(listed.content.starts_with("This session is api-9411. Others:\n"), "{}", listed.content);
    assert!(listed.content.contains("web-9412 (/work/web)"), "{}", listed.content);
    assert!(!listed.content.contains("api-9411 ("), "{}", listed.content);
    for client in [9411, 9412] {
        peers::forget(client);
    }
}

fn spec(name: &str, description: &str) -> ToolSpec {
    ToolSpec { name: name.into(), description: description.into(), schema: json!({"type": "object"}) }
}

fn jira() -> Deferred {
    let mut specs: Vec<ToolSpec> = (0..25).map(|n| spec(&format!("mcp__jira__tool{n}"), "")).collect();
    specs.extend([
        spec("mcp__jira__getJiraIssue", "Get a Jira issue by key. Reports how many comments it has."),
        spec("mcp__jira__editJiraIssue", "Edit an existing Jira issue. Change status with transitionJiraIssue."),
        spec("mcp__jira__transitionJiraIssue", "Move a Jira issue through its workflow."),
        spec("mcp__jira__addCommentToJiraIssue", "Add a comment to a Jira issue."),
    ]);
    defer(specs).1.expect("deferred")
}

fn searching(deferred: Deferred) -> Workspace {
    let mut workspace = Workspace::new(".");
    workspace.services.deferred = Some(Arc::new(deferred));
    workspace
}

#[test]
fn a_search_names_the_tools_still_waiting_so_the_next_one_can_select_them() {
    let workspace = searching(jira());
    let search = shipped("ToolSearch");
    assert_eq!(search.summary(&json!({"query": "get issue"})), "get issue");
    assert!(search.effects(&Value::Null, &workspace).is_empty());
    let outcome = search.run(&json!({"query": "get issue"}), &workspace);
    assert!(!outcome.is_error, "{}", outcome.content);
    assert!(outcome.content.contains("Not loaded yet"), "{}", outcome.content);
    assert!(outcome.content.contains("mcp__jira__transitionJiraIssue"), "{}", outcome.content);
    assert!(outcome.content.contains("select:"), "{}", outcome.content);
    let loaded = loaded_names(&outcome.content);
    assert_eq!(loaded[0], "mcp__jira__getJiraIssue", "{loaded:?}");
    assert!(loaded.iter().all(|name| workspace.services.deferred.as_ref().is_some_and(|d| d.is_loaded(name))));
}

#[test]
fn a_search_s_result_says_which_tools_it_loaded_in_the_order_found() {
    let workspace = searching(jira());
    let search = shipped("ToolSearch");
    let selected = search.run(&json!({"query": "select:mcp__jira__tool9, mcp__jira__tool3"}), &workspace);
    assert_eq!(loaded_names(&selected.content), ["mcp__jira__tool3", "mcp__jira__tool9"]);
    let parameters = "  {\n    \"name\": \"mcp__jira__tool3\",\n    \"description\": \"\",\n    \"parameters\": {";
    assert!(selected.content.contains(parameters), "{}", selected.content);
    let loaded: Vec<String> = workspace
        .services
        .deferred
        .as_ref()
        .map(|d| d.loaded())
        .unwrap_or_default()
        .into_iter()
        .map(|s| s.name)
        .collect();
    assert_eq!(loaded, ["mcp__jira__tool3", "mcp__jira__tool9"]);
    let ranked = search.run(&json!({"query": "transition de issue e jira"}), &workspace);
    assert_eq!(loaded_names(&ranked.content)[0], "mcp__jira__transitionJiraIssue", "{}", ranked.content);
    assert_eq!(loaded_names(&ranked.content).len(), 5);
    let short = search.run(&json!({"query": "e de"}), &workspace);
    assert_eq!((short.content.as_str(), short.is_error), ("No tool matches \"e de\".", true));
    assert!(loaded_names(&short.content).is_empty());
}

#[test]
fn a_search_with_everything_loaded_or_no_query_says_so() {
    let search = shipped("ToolSearch");
    let everything = search.run(&json!({"query": "x"}), &Workspace::new("."));
    assert_eq!((everything.content.as_str(), everything.is_error), ("Every tool is already loaded.", true));
    let missing = search.run(&json!({}), &searching(jira()));
    assert_eq!(missing.content, "Missing required parameter `query`.");
    let full = searching(jira());
    let every: Vec<String> = (0..25).map(|n| format!("mcp__jira__tool{n}")).collect();
    let names = ["getJiraIssue", "editJiraIssue", "transitionJiraIssue", "addCommentToJiraIssue"];
    let all = every.into_iter().chain(names.map(|name| format!("mcp__jira__{name}"))).collect::<Vec<_>>().join(",");
    let selected = search.run(&json!({"query": format!("select:{all}")}), &full);
    assert!(!selected.content.contains("Not loaded yet"), "{}", selected.content);
    assert_eq!(loaded_names(&selected.content).len(), 29);
}

#[test]
fn the_tool_search_handle_runs_the_shipped_script() {
    let mut workspace = Workspace::new(std::env::temp_dir());
    let search = super::shipped("ToolSearch");
    assert!(!search.offered(&workspace), "offered only when deferring");
    let specs = vec![crate::model::ToolSpec {
        name: "mcp__a__b".into(),
        description: "d".repeat(50_000),
        schema: json!({"type": "object"}),
    }];
    workspace.services.deferred = crate::tools::tool_search::defer(specs).1.map(Arc::new);
    assert!(search.offered(&workspace) == workspace.services.deferred.is_some());
    assert!(crate::tools::standard().iter().any(|tool| tool.spec().name == "ListSessions"));
    assert!(crate::tools::standard().iter().any(|tool| tool.spec().name == "ScheduleWakeup"));
}

fn project(server: &Arc<ScriptedServer>) -> (tempfile::TempDir, Workspace) {
    let dir = tempfile::tempdir().expect("dir");
    std::fs::create_dir_all(dir.path().join("src")).expect("src");
    std::fs::write(dir.path().join("src/main.rs"), "mod tools;\nfn main() {\n    tools::run(\"é\", go);\n}\n")
        .expect("main");
    std::fs::write(dir.path().join("src/tools.rs"), "/// Runs.\npub fn run(a: &str, b: u8) {}\n").expect("tools");
    let mut workspace = Workspace::new(dir.path());
    workspace.services.lsp = Some(server.lsp());
    (dir, workspace)
}

fn range(line: u32, character: u32) -> Value {
    json!({"start": {"line": line, "character": character}, "end": {"line": line, "character": character + 3}})
}

#[test]
fn a_definition_is_asked_at_the_position_read_shows_and_answered_as_a_place() {
    let server = ScriptedServer::answering([Ok(json!([]))]);
    let (dir, workspace) = project(&server);
    let uri = domain::lsp::uri::uri_of(&dir.path().join("src/tools.rs"));
    *server.answers.lock().expect("answers") = [Ok(json!({"uri": uri, "range": range(1, 7)}))].into();
    let input = json!({"operation": "goToDefinition", "file_path": "src/main.rs", "line": 3, "character": 18});
    let outcome = super::shipped("LSP").run(&input, &workspace);
    assert!(!outcome.is_error, "{}", outcome.content);
    assert_eq!(outcome.content, "src/tools.rs:2:8: pub fn run(a: &str, b: u8) {}");
    let (method, params) = &server.asked()[0];
    assert_eq!(method, "textDocument/definition");
    assert_eq!(params["position"], json!({"line": 2, "character": 17}));
    let synced = server.synced.lock().expect("synced");
    assert_eq!((synced[0].1.as_str(), synced[0].2.contains("tools::run")), ("rust", true));
}

#[test]
fn calls_are_asked_in_two_steps() {
    let item =
        json!({"name": "run", "kind": 12, "uri": "file:///x.rs", "range": range(1, 0), "selectionRange": range(1, 7)});
    let server = ScriptedServer::answering([Ok(json!([item])), Ok(json!([]))]);
    let (_dir, workspace) = project(&server);
    let input = json!({"operation": "incomingCalls", "file_path": "src/tools.rs", "line": 2, "character": 8});
    assert_eq!(super::shipped("LSP").run(&input, &workspace).content, "No callers found.");
    let methods: Vec<String> = server.asked().into_iter().map(|(method, _)| method).collect();
    assert_eq!(methods, ["textDocument/prepareCallHierarchy", "callHierarchy/incomingCalls"]);
}

fn problem(severity: u64, line: u32, message: &str) -> Value {
    json!({"severity": severity, "message": message, "range": range(line, 4)})
}

#[test]
fn a_file_s_diagnostics_are_asked_after_syncing_it_and_said_compactly() {
    let server = ScriptedServer::answering([]);
    let (_dir, workspace) = project(&server);
    *server.diagnostics.lock().expect("diagnostics") = Some(vec![problem(2, 0, "unused"), problem(1, 2, "no `go`")]);
    let asked = json!({"operation": "diagnostics", "file_path": "src/main.rs"});
    let outcome = super::shipped("LSP").run(&asked, &workspace);
    assert!(!outcome.is_error, "{}", outcome.content);
    assert_eq!(
        outcome.content,
        "1 error and 1 warning in 1 file\nsrc/main.rs:3:5 error: no `go`\nsrc/main.rs:1:5 warning: unused"
    );
    assert!(server.synced.lock().expect("synced")[0].2.contains("tools::run"), "the file on disk is sent first");
    assert!(server.asked().is_empty(), "diagnostics are read from what the server published");

    *server.diagnostics.lock().expect("diagnostics") = Some(Vec::new());
    assert_eq!(super::shipped("LSP").run(&asked, &workspace).content, "No errors or warnings in src/main.rs.");
    *server.diagnostics.lock().expect("diagnostics") = None;
    let silent = super::shipped("LSP").run(&asked, &workspace);
    assert!(!silent.is_error && silent.content.contains("may still be checking"), "{}", silent.content);
}

#[test]
fn the_workspace_s_diagnostics_are_every_file_the_server_reported_on() {
    let server = ScriptedServer::answering([]);
    let (dir, workspace) = project(&server);
    *server.published.lock().expect("published") = vec![
        (domain::lsp::uri::uri_of(&dir.path().join("src/tools.rs")), vec![problem(2, 1, "unused `b`")]),
        (domain::lsp::uri::uri_of(&dir.path().join("src/main.rs")), vec![problem(1, 2, "no `go`")]),
        ("file:///elsewhere/lib.rs".into(), vec![problem(4, 0, "a hint")]),
    ];
    let asked = json!({"operation": "workspaceDiagnostics", "file_path": "src/main.rs"});
    assert_eq!(
        super::shipped("LSP").run(&asked, &workspace).content,
        "1 error and 1 warning in 2 files\nsrc/main.rs:3:5 error: no `go`\nsrc/tools.rs:2:5 warning: unused `b`"
    );
    server.published.lock().expect("published").clear();
    assert!(
        super::shipped("LSP")
            .run(&asked, &workspace)
            .content
            .starts_with("The language server reports no errors or warnings")
    );
}

#[test]
fn asking_for_diagnostics_installs_a_missing_server() {
    let server = ScriptedServer::answering([]);
    let (_dir, workspace) = project(&server);
    *server.missing.lock().expect("missing") = Some("install rust-analyzer".into());
    *server.diagnostics.lock().expect("diagnostics") = Some(Vec::new());
    let asked = json!({"operation": "diagnostics", "file_path": "src/main.rs"});
    let effects = super::shipped("LSP").effects(&asked, &workspace);
    assert!(effects.iter().any(|effect| matches!(effect, crate::tools::Effect::Install { .. })), "asks first");
    let installed = super::shipped("LSP").run(&asked, &workspace);
    assert!(installed.content.starts_with("Installed the "), "{}", installed.content);
    assert!(installed.content.ends_with("No errors or warnings in src/main.rs."), "{}", installed.content);
    assert_eq!(server.installs.load(std::sync::atomic::Ordering::SeqCst), 1);
}

#[test]
fn what_cannot_be_asked_is_an_error_that_says_why() {
    let server =
        ScriptedServer::answering([Err(domain::lsp::answer::RequestError::Busy("fake: content modified".into()))]);
    let (_dir, workspace) = project(&server);
    let ask = |input: Value| super::shipped("LSP").run(&input, &workspace);
    let busy = ask(json!({"operation": "hover", "file_path": "src/main.rs", "line": 1, "character": 1}));
    assert!(busy.is_error && busy.content.contains("still indexing"), "{}", busy.content);
    assert!(busy.content.contains("Do not repeat this identical call right away"), "{}", busy.content);
    let missing = ask(json!({"operation": "documentSymbol", "file_path": "src/gone.rs"}));
    assert!(missing.is_error && missing.content.contains("src/gone.rs"), "{}", missing.content);
    let unknown = ask(json!({"operation": "documentSymbol", "file_path": "README"}));
    assert!(unknown.is_error, "{}", unknown.content);
    let bad = ask(json!({"operation": "rename", "file_path": "src/main.rs"}));
    assert!(bad.is_error && bad.content.contains("goToDefinition"), "{}", bad.content);
    let without = super::shipped("LSP")
        .run(&json!({"operation": "documentSymbol", "file_path": "src/main.rs"}), &Workspace::new("/w"));
    assert!(without.is_error, "{}", without.content);
}
