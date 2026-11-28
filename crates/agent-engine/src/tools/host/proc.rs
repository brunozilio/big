mod output;

use std::process::Stdio;
use std::time::Duration;

use serde_json::{Value, json};

use super::{Reach, text};
use crate::tools::builtin::shell::shells::{self, Waited, Watch};

pub const SPACE: &str = "proc";

pub fn call(reach: &Reach<'_>, name: &str, arguments: &Value) -> Result<Value, String> {
    match name {
        "spawn" => spawn(reach, arguments),
        "wait" => wait(reach, arguments),
        "watch" => watch(reach, arguments),
        "read" => read(reach, arguments),
        "kill" => kill(reach, arguments),
        "list" => Ok(list(reach)),
        other => output::call(reach, other, arguments),
    }
}

fn flag(arguments: &Value, key: &str) -> bool {
    arguments.get(key).and_then(Value::as_bool) == Some(true)
}

fn spawn(reach: &Reach<'_>, arguments: &Value) -> Result<Value, String> {
    let command = text(arguments, "command")?;
    reach.fence.runs(command)?;
    let workspace = reach.workspace;
    let mut shell = workspace.launched(platform::shell(command)).map_err(|error| error.to_string())?;
    shell.current_dir(&workspace.cwd).stdin(Stdio::null());
    if flag(arguments, "git") {
        let inherited = std::env::var("GIT_CONFIG_COUNT").ok();
        shell.envs(platform::git::environment(&workspace.cwd, inherited.as_deref(), platform::git::Run::Approved));
    }
    if flag(arguments, "plain") {
        shell.envs(domain::output::PLAIN_ENV);
    }
    shells::start(workspace, command, &mut shell).map(|id| json!({ "id": id })).map_err(|error| error.to_string())
}

fn owned<'v>(reach: &Reach<'_>, arguments: &'v Value) -> Result<&'v str, String> {
    let id = text(arguments, "id")?;
    if shells::owned(reach.workspace.credentials.client, id) {
        Ok(id)
    } else {
        Err(format!("No background command `{id}`."))
    }
}

fn wait(reach: &Reach<'_>, arguments: &Value) -> Result<Value, String> {
    let id = owned(reach, arguments)?;
    let timeout = arguments.get("timeout").and_then(Value::as_u64).ok_or("timeout must be a number of milliseconds")?;
    let snapshot = arguments.get("changes").and_then(Value::as_u64);
    let workspace = reach.workspace;
    let waited = shells::wait(id, Duration::from_millis(timeout), &workspace.stop, &workspace.detach);
    let not_restorable = snapshot.and_then(|snapshot| super::git::since(workspace, snapshot).ok());
    let mut answer = match waited {
        Waited::Exited { output, success, code } => {
            json!({ "ended": "exited", "output": output, "success": success, "code": code })
        }
        Waited::Detached => json!({ "ended": "detached" }),
        Waited::TimedOut(output) => json!({ "ended": "timedOut", "output": output }),
        Waited::Stopped(output) => json!({ "ended": "stopped", "output": output }),
    };
    answer["notRestorable"] = json!(not_restorable.unwrap_or_default());
    Ok(answer)
}

fn watch(reach: &Reach<'_>, arguments: &Value) -> Result<Value, String> {
    let id = owned(reach, arguments)?;
    let what = if flag(arguments, "lines") { Watch::Lines } else { Watch::End };
    shells::watch(id.to_owned(), reach.workspace.credentials.client, what);
    Ok(Value::Null)
}

fn read(reach: &Reach<'_>, arguments: &Value) -> Result<Value, String> {
    let id = text(arguments, "id")?;
    Ok(shells::read(reach.workspace.credentials.client, id).map_or(
        Value::Null,
        |read| json!({ "text": read.text, "dropped": read.dropped, "exit": read.exit, "seconds": read.seconds }),
    ))
}

fn kill(reach: &Reach<'_>, arguments: &Value) -> Result<Value, String> {
    let id = text(arguments, "id")?;
    let client = reach.workspace.credentials.client;
    let stopped = crate::tools::watch::stop(client, id).or_else(|| shells::stopped(client, id));
    Ok(stopped.map_or(Value::Null, |what| json!({ "stopped": what })))
}

fn list(reach: &Reach<'_>) -> Value {
    let listed = shells::list(reach.workspace.credentials.client);
    let listed =
        listed.into_iter().map(|(id, command, running)| json!({ "id": id, "command": command, "running": running }));
    Value::Array(listed.collect())
}

#[cfg(all(test, unix))]
mod tests {
    use super::super::Fence;
    use super::*;
    use crate::tools::{Effect, Workspace};

    #[test]
    fn a_shell_runs_only_the_declared_command_and_only_its_client_reaches_it() {
        let mut workspace = Workspace::new(std::env::temp_dir());
        workspace.credentials.client = 78_001;
        let effects = [Effect::exec("echo hi; sleep 30")];
        let reach = Reach { workspace: &workspace, fence: Fence::of(&effects), streams: None };
        assert!(call(&reach, "spawn", &json!({ "command": "echo other" })).is_err());
        let id = call(&reach, "spawn", &json!({ "command": "echo hi; sleep 30" })).expect("spawn")["id"].clone();
        let listed = call(&reach, "list", &Value::Null).expect("list");
        assert_eq!(listed, json!([{ "id": id, "command": "echo hi; sleep 30", "running": true }]));
        let mut other = Workspace::new(std::env::temp_dir());
        other.credentials.client = 78_002;
        let stranger = Reach { workspace: &other, fence: Fence::of(&effects), streams: None };
        assert_eq!(call(&stranger, "read", &json!({ "id": id })), Ok(Value::Null));
        assert!(call(&stranger, "wait", &json!({ "id": id, "timeout": 10 })).is_err());
        assert_eq!(call(&stranger, "kill", &json!({ "id": id })), Ok(Value::Null));
        let waited = call(&reach, "wait", &json!({ "id": id, "timeout": 200 })).expect("wait");
        assert_eq!(waited["ended"], "timedOut");
        assert!(waited["output"].as_str().is_some_and(|output| output.contains("hi")), "{waited}");
        assert_eq!(waited["notRestorable"], json!([]));
        assert_eq!(call(&reach, "list", &Value::Null), Ok(json!([])));
    }
}
