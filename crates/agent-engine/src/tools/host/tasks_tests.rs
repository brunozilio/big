use serde_json::json;

use crate::tools::{Outcome, Workspace};
use domain::team::Protocol;

fn runner(workspace: &Workspace) -> impl Fn(&str, serde_json::Value) -> Outcome + '_ {
    let tools = crate::tools::standard();
    move |name: &str, input: serde_json::Value| {
        let tool = tools.iter().find(|tool| tool.spec().name == name).expect("valid test fixture");
        assert!(tool.script().is_some(), "{name} is a script tool");
        tool.run(&input, workspace)
    }
}

#[test]
fn planning_tasks_track_dependencies_and_reject_cycles_atomically() {
    let workspace = Workspace::new(".");
    let run = runner(&workspace);
    assert!(!run("TaskCreate", json!({"subject":"Build"})).is_error);
    assert!(!run("TaskCreate", json!({"subject":"Verify"})).is_error);
    assert!(!run("TaskUpdate", json!({"task_id":"2","blocked_by":["1"]})).is_error);
    assert!(run("TaskUpdate", json!({"task_id":"1","blocked_by":["2"]})).is_error);
    assert!(run("TaskUpdate", json!({"task_id":"2","status":"completed"})).is_error);
    assert!(!run("TaskUpdate", json!({"task_id":"1","status":"completed"})).is_error);
    assert!(!run("TaskUpdate", json!({"task_id":"2","status":"completed"})).is_error);
}

#[test]
fn a_change_that_does_not_read_is_refused_before_the_board_is_touched() {
    let workspace = Workspace::new(".");
    let run = runner(&workspace);
    assert!(!run("TaskCreate", json!({"subject":"Build"})).is_error);
    let status = run("TaskUpdate", json!({"task_id":"1","status":"done"}));
    assert!(status.is_error && status.content == "status is pending, in_progress or completed", "{status:?}");
    let blocked = run("TaskUpdate", json!({"task_id":"1","blocked_by":"2"}));
    assert_eq!(blocked.content, "blocked_by must be an array of task ids");
    let described = run("TaskUpdate", json!({"task_id":"1","description":3}));
    assert_eq!(described.content, "description must be text");
    assert!(run("TaskGet", json!({"task_id":"1"})).content.contains("\"status\":\"pending\""));
}

#[cfg(unix)]
#[test]
fn a_task_hook_can_refuse_a_new_task_or_its_completion_and_says_why() {
    let dir = tempfile::tempdir().expect("dir");
    let mut workspace = Workspace::new(dir.path());
    let groups = json!({
        "TaskCreated": [{"hooks": [{"type": "command", "command": "grep -q forbidden && { printf 'no forbidden tasks' >&2; exit 2; }; exit 0"}]}],
        "TaskCompleted": [{"hooks": [{"type": "command", "command": "printf 'run the tests first' >&2; exit 2"}]}]
    });
    workspace.hooks = crate::hooks::Hooks::parse(&groups).expect("hooks");
    let run = runner(&workspace);

    let refused = run("TaskCreate", json!({"subject":"a forbidden thing"}));
    let made = run("TaskCreate", json!({"subject":"Build"}));
    let unfinished = run("TaskUpdate", json!({"task_id":"1","status":"completed"}));
    let started = run("TaskUpdate", json!({"task_id":"1","status":"in_progress"}));

    assert!(refused.is_error && refused.content.contains("no forbidden tasks"), "{}", refused.content);
    assert!(!made.is_error && made.content.contains("\"task_id\":\"1\""), "{}", made.content);
    assert!(unfinished.is_error && unfinished.content.contains("run the tests first"), "{}", unfinished.content);
    assert!(!started.is_error, "only completing is hooked: {}", started.content);
    assert!(run("TaskGet", json!({"task_id":"1"})).content.contains("\"status\":\"in_progress\""));
}

#[test]
fn with_a_home_the_list_is_kept_for_the_session_and_survives_the_run() {
    let home = tempfile::tempdir().expect("home");
    let mut workspace = Workspace::new(".");
    workspace.home = Some(home.path().to_path_buf());
    workspace.session_id = "lead-7".into();
    assert!(!runner(&workspace)("TaskCreate", json!({"subject":"Build"})).is_error);

    let listed = super::listed(&workspace).expect("listed");

    assert!(listed.iter().any(|task| task["subject"] == "Build"), "a new run reads it: {listed:?}");
    assert!(crate::team::tasks_file(home.path(), "lead-7").is_file());
}

#[test]
fn the_lead_assigning_a_task_to_a_teammate_tells_the_teammate() {
    let home = tempfile::tempdir().expect("home");
    let mut workspace = Workspace::new(".");
    workspace.home = Some(home.path().to_path_buf());
    workspace.session_id = "lead-8".into();
    let team = crate::team::Team::of_session(home.path(), "lead-8");
    team.open("lead-8").expect("team");
    team.join("tester", domain::team::Member::default()).expect("join");
    let run = runner(&workspace);
    assert!(!run("TaskCreate", json!({"subject":"Fix lint"})).is_error);

    let assigned = run("TaskUpdate", json!({"task_id":"1","owner":"tester"}));

    assert!(assigned.content.contains("\"owner\":\"tester\""), "{}", assigned.content);
    let mail = team.unread("tester");
    assert!(matches!(
        Protocol::of(&mail[0].text),
        Some(Protocol::TaskAssignment { task_id, assigned_by, .. }) if task_id == "1" && assigned_by == "team-lead"
    ));
}
