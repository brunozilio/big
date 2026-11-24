use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

use domain::task_board::{Assigned, Board, Change, Status};
use domain::team::Protocol;
use platform::store::Document;
use platform::sync::Locked;
use serde_json::{Value, json};

use super::Reach;
use crate::hooks::HookEvent;
use crate::tools::Workspace;

pub const SPACE: &str = "tasks";

pub fn call(reach: &Reach<'_>, name: &str, arguments: &Value) -> Result<Value, String> {
    let workspace = reach.workspace;
    match name {
        "list" => listed(workspace).map(Value::Array),
        "create" => create(workspace, arguments),
        "get" => on_board(workspace, |board| board.get(arguments.as_str().unwrap_or_default())),
        "update" => update(workspace, arguments),
        other => Err(format!("ctx.{SPACE}.{other} is not a function")),
    }
}

pub(crate) fn listed(workspace: &Workspace) -> Result<Vec<Value>, String> {
    on_board(workspace, |board| Ok(board.listed()))
}

fn kept_here() -> &'static Mutex<HashMap<String, Board>> {
    static BOARDS: OnceLock<Mutex<HashMap<String, Board>>> = OnceLock::new();
    BOARDS.get_or_init(Mutex::default)
}

fn shared(workspace: &Workspace) -> Option<PathBuf> {
    crate::team::task_list(workspace.home.as_deref(), &workspace.session_id, workspace.teammate.as_ref())
}

fn me(workspace: &Workspace) -> String {
    crate::team::current(workspace.home.as_deref(), &workspace.session_id, workspace.teammate.as_ref())
        .map_or_else(|| domain::team::LEAD.to_owned(), |me| me.name)
}

fn on_board<R>(workspace: &Workspace, work: impl FnOnce(&mut Board) -> Result<R, String>) -> Result<R, String> {
    match shared(workspace) {
        Some(path) => Document::at(path).try_update(work).map_err(|error| error.to_string())?,
        None => work(kept_here().locked().entry(workspace.session_id.clone()).or_default()),
    }
}

fn hooked(workspace: &Workspace, event: HookEvent, task: Value) -> Result<(), String> {
    let input = json!({ "task": task, "teammate_name": me(workspace) });
    let ran = workspace.hooks.run(event, "", input, workspace)?;
    match ran.blocked {
        Some(reason) => Err(format!("A {} hook refused it: {reason}", event.as_str())),
        None => Ok(()),
    }
}

fn create(workspace: &Workspace, arguments: &Value) -> Result<Value, String> {
    let proposed = json!({ "subject": arguments["subject"], "description": arguments["description"] });
    hooked(workspace, HookEvent::TaskCreated, proposed)?;
    on_board(workspace, |board| {
        let subject = arguments["subject"].as_str().unwrap_or_default();
        let description = arguments["description"].as_str().unwrap_or_default();
        board.create(subject, description, arguments["activeForm"].as_str()).map(|task| json!(task))
    })
}

#[derive(serde::Deserialize)]
struct Asked {
    task_id: String,
    status: Option<String>,
    description: Option<String>,
    blocked_by: Option<Vec<String>>,
    owner: Option<String>,
}

fn update(workspace: &Workspace, arguments: &Value) -> Result<Value, String> {
    let asked: Asked = serde_json::from_value(arguments.clone()).map_err(|error| error.to_string())?;
    let status = match asked.status.as_deref() {
        Some(name) => Some(Status::named(name).ok_or("status is pending, in_progress or completed")?),
        None => None,
    };
    if status == Some(Status::Completed) {
        let task = on_board(workspace, |board| board.get(&asked.task_id))?;
        hooked(workspace, HookEvent::TaskCompleted, task)?;
    }
    let change = Change { status, description: asked.description, blocked_by: asked.blocked_by, owner: asked.owner };
    let by = me(workspace);
    let (task, assigned) = on_board(workspace, |board| board.update(&asked.task_id, change, &by))?;
    if let Some(assigned) = assigned {
        told(workspace, assigned, &by);
    }
    Ok(task)
}

fn told(workspace: &Workspace, assigned: Assigned, by: &str) {
    let Some(me) = crate::team::current(workspace.home.as_deref(), &workspace.session_id, workspace.teammate.as_ref())
    else {
        return;
    };
    if !me.team.knows(&assigned.owner) {
        return;
    }
    let message = Protocol::TaskAssignment {
        task_id: assigned.task_id,
        subject: assigned.subject.clone(),
        assigned_by: by.to_owned(),
    };
    let _ = me.team.send(&assigned.owner, by, &message.text(), Some(&format!("task: {}", assigned.subject)));
}

#[cfg(test)]
#[path = "tasks_tests.rs"]
mod tests;
