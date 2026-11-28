mod artifacts;
mod crypto;
mod fence;
mod files;
mod fs;
mod git;
mod history;
mod hosted;
mod http;
mod lsp;
mod mcp;
mod media;
mod observer;
mod proc;
mod schema;
mod search;
mod session;
pub(crate) mod tasks;
mod worklog;

use serde_json::Value;

pub use fence::Fence;

use super::Workspace;

#[derive(Debug)]
pub struct Reach<'a> {
    pub workspace: &'a Workspace,
    pub fence: Fence<'a>,
    pub streams: Option<&'a crate::model::script_http::Streams>,
}

type Space = fn(&Reach<'_>, &str, &Value) -> Result<Value, String>;

const SPACES: &[(&str, Space)] = &[
    (fs::SPACE, fs::call),
    (files::SPACE, files::call),
    (search::SPACE, search::call),
    (media::SPACE, media::call),
    (proc::SPACE, proc::call),
    (git::SPACE, git::call),
    (http::SPACE, http::call),
    (crypto::SPACE, crypto::call),
    (lsp::SPACE, lsp::call),
    (mcp::SPACE, mcp::call),
    (session::SPACE, session::call),
    (history::SPACE, history::call),
    (worklog::SPACE, worklog::call),
    (schema::SPACE, schema::call),
    (tasks::SPACE, tasks::call),
    (observer::SPACE, observer::call),
    (hosted::SPACE, hosted::call),
    (artifacts::SPACE, artifacts::call),
];

fn space(function: &str) -> Option<(Space, &str)> {
    let (name, rest) = function.split_once('.')?;
    SPACES.iter().find(|(known, _)| *known == name).map(|(_, space)| (*space, rest))
}

pub fn named(function: &str) -> bool {
    space(function).is_some()
}

pub fn call(reach: &Reach<'_>, function: &str, arguments: &Value) -> Result<Value, String> {
    match space(function) {
        Some((space, name)) => space(reach, name, arguments),
        None => Err(format!("ctx.{function} is not a function")),
    }
}

pub fn run_alone(script: &super::script_tool::ScriptTool, input: &Value, workspace: &Workspace) -> super::Outcome {
    let effects = super::Tool::effects(script, input, workspace);
    let streams = crate::model::script_http::Streams::default();
    let reach = Reach { workspace, fence: Fence::of(&effects), streams: Some(&streams) };
    let data = serde_json::json!({ "cwd": workspace.cwd.display().to_string() });
    let stop = workspace.stop.flag();
    let (path, source, input) = (script.shown_path(), script.source.clone(), input.clone());
    let result = super::script_bridge::hosted(
        "the tool crashed",
        move |host| script::run(&path, &source, "execute", &[input], &data, host, stop),
        |function, arguments| call(&reach, function, &arguments),
    );
    match result {
        Ok(value) => super::contract::outcome(value),
        Err(error) => super::Outcome::error(format!("{} failed: {error}", script.spec.name)),
    }
}

pub(crate) fn text<'v>(arguments: &'v Value, key: &str) -> Result<&'v str, String> {
    arguments.get(key).and_then(Value::as_str).ok_or_else(|| format!("{key} must be a string"))
}
