use std::path::Path;

use serde_json::{Value, json};

use super::*;

fn written(dir: &Path, name: &str, source: &str) -> String {
    let file = dir.join(name);
    std::fs::write(&file, source).expect("interceptor");
    file.display().to_string()
}

fn one(run: String, matcher: &str, timeout: u64) -> Interceptor {
    Interceptor::new(run, matcher, timeout, Level::Global)
}

fn ran(
    interceptors: &Interceptors,
    subject: &str,
    value: Value,
    workspace: &Workspace,
    terminal: &mut Terminal<'_>,
) -> Result<Value, String> {
    let chain = Chain { kind: Kind::Tool, subject, workspace, model: None, terminal };
    interceptors.run(value, chain)
}

const TRAIL: &str = r#"export const intercept = {
  tool: [
    { run(value: any, ctx: any, next: any) {
      const answer = next({ trail: [...value.trail, "outer"] });
      return { trail: [...answer.trail, "outer"], agent: ctx.agent };
    } },
    { run(value: any, ctx: any, next: any) {
      const answer = next({ trail: [...value.trail, "inner"] });
      return { trail: [...answer.trail, "inner"] };
    } },
  ],
};"#;

#[test]
fn layers_wrap_one_another_from_the_outside_in_and_the_terminal_runs_last() {
    let dir = tempfile::tempdir().expect("dir");
    let file = written(dir.path(), "agent.ts", TRAIL);
    let interceptors = Interceptors {
        tool: vec![
            one(format!("{file}#intercept.tool.0.run"), "", 5),
            one(format!("{file}#intercept.tool.1.run"), "", 5),
        ],
        model: Vec::new(),
    };
    let mut workspace = Workspace::new(dir.path());
    workspace.agent_name = "dev".into();
    let mut terminal = |value: Value| {
        let mut trail = value["trail"].as_array().cloned().unwrap_or_default();
        trail.push(json!("tool"));
        Ok(json!({ "trail": trail }))
    };
    let answer = ran(&interceptors, "Bash", json!({ "trail": [] }), &workspace, &mut terminal).expect("ran");
    assert_eq!(answer["trail"], json!(["outer", "inner", "tool", "inner", "outer"]));
    assert_eq!(answer["agent"], "dev");
}

#[test]
fn an_interceptor_may_answer_without_calling_next_and_only_the_matching_ones_run() {
    let dir = tempfile::tempdir().expect("dir");
    let file = written(
        dir.path(),
        "block.js",
        "export default (value) => ({ content: `no ${value.command}`, isError: true });",
    );
    let interceptors = Interceptors { tool: vec![one(file, "Bash", 5)], model: Vec::new() };
    let workspace = Workspace::new(dir.path());
    let mut calls = 0;
    let mut terminal = |value: Value| {
        calls += 1;
        Ok(value)
    };
    let blocked = ran(&interceptors, "Bash", json!({ "command": "rm" }), &workspace, &mut terminal).expect("ran");
    assert_eq!(blocked, json!({ "content": "no rm", "isError": true }));
    let passed = ran(&interceptors, "BashOutput", json!({ "command": "rm" }), &workspace, &mut terminal).expect("ran");
    assert_eq!(passed, json!({ "command": "rm" }));
    assert_eq!(calls, 1, "the matcher names the whole tool, so BashOutput reaches the tool");
}

const RETRY: &str = r#"export default function (value, ctx, next) {
  try {
    return next();
  } catch (error) {
    return { retried: next({ ...value, again: true }), first: String(error.message) };
  }
}"#;

#[test]
fn next_may_be_called_again_after_it_failed_and_its_failure_reaches_the_script() {
    let dir = tempfile::tempdir().expect("dir");
    let file = written(dir.path(), "retry.js", RETRY);
    let interceptors = Interceptors { tool: vec![one(file, "", 5)], model: Vec::new() };
    let workspace = Workspace::new(dir.path());
    let mut calls = 0;
    let mut terminal = |value: Value| {
        calls += 1;
        if value["again"] == true { Ok(json!("ok")) } else { Err("flaky".to_owned()) }
    };
    let answer = ran(&interceptors, "Bash", json!({}), &workspace, &mut terminal).expect("ran");
    assert_eq!(answer, json!({ "retried": "ok", "first": "flaky" }));
    assert_eq!(calls, 2);
}

#[test]
fn a_throwing_a_looping_and_a_slow_interceptor_each_fail_with_their_reason() {
    let dir = tempfile::tempdir().expect("dir");
    let throws = written(dir.path(), "throws.js", "export default () => { throw new Error(\"nope\"); };");
    let loops = written(dir.path(), "loops.js", "export default (value, ctx, next) => { for (;;) next(); };");
    let slow = written(dir.path(), "slow.js", "export default () => { for (;;) {} };");
    let workspace = Workspace::new(dir.path());
    let mut terminal = |value: Value| Ok(value);
    for (file, reason) in [(throws, "nope"), (loops, "more than 16 times"), (slow, "running time")] {
        let interceptors = Interceptors { tool: vec![one(file.clone(), "", 1)], model: Vec::new() };
        let error = ran(&interceptors, "Bash", json!({}), &workspace, &mut terminal).expect_err("failed");
        assert!(error.starts_with(&file) && error.contains(reason), "{error}");
    }
    assert!(!workspace.stop.load(std::sync::atomic::Ordering::Relaxed), "the run itself goes on");
}

#[test]
fn a_bad_timeout_or_matcher_is_named_when_read() {
    let bad_timeout = Interceptors::parse(&json!({ "tool": [{ "run": "a.js", "timeout": 0 }] }));
    assert!(bad_timeout.is_err_and(|error| error.contains("outside 1..600")));
    let bad_matcher = Interceptors::parse(&json!({ "model": [{ "run": "a.js", "matcher": "(" }] }));
    assert!(bad_matcher.is_err_and(|error| error.contains("not a regex")));
    let unknown = Interceptors::parse(&json!({ "tools": [] }));
    assert!(unknown.is_err());
    let read = Interceptors::parse(&json!({ "model": [{ "run": "a.js" }] })).expect("read");
    assert!(read.listens_to(Kind::Model) && !read.listens_to(Kind::Tool));
    assert_eq!(read.model[0].timeout, 60);
}

#[test]
fn the_global_layer_wraps_the_agent_s_which_wraps_the_tool_s_whatever_order_they_were_added_in() {
    let dir = tempfile::tempdir().expect("dir");
    let mut tool = Vec::new();
    for (name, level) in [("tool", Level::Tool), ("agent", Level::Agent), ("global", Level::Global)] {
        let source = format!(
            "export default (value, ctx, next) => ({{ trail: [\"{name}\", ...next({{ trail: [...value.trail, \"{name}\"] }}).trail] }});"
        );
        let file = written(dir.path(), &format!("{name}.js"), &source);
        tool.push(Interceptor::new(file, "", 5, level));
    }
    let interceptors = Interceptors { tool, model: Vec::new() };
    let workspace = Workspace::new(dir.path());
    let mut terminal = |value: Value| Ok(json!({ "trail": [value["trail"].clone()] }));
    let answer = ran(&interceptors, "Bash", json!({ "trail": [] }), &workspace, &mut terminal).expect("ran");
    assert_eq!(answer["trail"], json!(["global", "agent", "tool", ["global", "agent", "tool"]]));
}
