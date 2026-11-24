use domain::inbox::Notice;
use serde_json::{Value, json};

use super::{Reach, text};
use crate::tools::builtin::schedules;
use crate::tools::{inbox, peers};

pub const SPACE: &str = "sessions";

pub fn call(reach: &Reach<'_>, name: &str, arguments: &Value) -> Result<Value, String> {
    match name {
        "schedule" => schedule(reach, arguments),
        "peers" => Ok(listed_peers(reach)),
        "inbox.push" => push(reach, arguments),
        "inbox.withdraw" => withdraw(reach, arguments),
        "deferred" => Ok(deferred(reach)),
        "load" => load(reach, arguments),
        other => Err(format!("ctx.{SPACE}.{other} is not a function")),
    }
}

fn client(reach: &Reach<'_>, arguments: &Value) -> u64 {
    arguments.get("to").and_then(Value::as_u64).unwrap_or(reach.workspace.credentials.client)
}

fn schedule(reach: &Reach<'_>, arguments: &Value) -> Result<Value, String> {
    reach.fence.spawns(crate::tools::SpawnKind::Schedule)?;
    let prompt = text(arguments, "prompt")?;
    let after = arguments.get("after").and_then(Value::as_i64).ok_or("after must be a whole number of seconds")?;
    let scheduled = schedules::add(reach.workspace.credentials.client, prompt, schedules::now() + after)?;
    Ok(json!({ "id": scheduled.id, "at": scheduled.next, "local": schedules::local_minute(scheduled.next) }))
}

fn listed_peers(reach: &Reach<'_>) -> Value {
    let me = reach.workspace.credentials.client;
    let listed: Vec<Value> = peers::open()
        .into_iter()
        .map(|peer| {
            json!({
                "client": peer.client,
                "name": peer.name,
                "cwd": peer.cwd.display().to_string(),
                "me": peer.client == me,
            })
        })
        .collect();
    Value::Array(listed)
}

fn push(reach: &Reach<'_>, arguments: &Value) -> Result<Value, String> {
    let notice = Notice::new(text(arguments, "source")?, text(arguments, "text")?);
    inbox::push(client(reach, arguments), notice);
    Ok(Value::Null)
}

fn withdraw(reach: &Reach<'_>, arguments: &Value) -> Result<Value, String> {
    inbox::withdraw(client(reach, arguments), text(arguments, "source")?);
    Ok(Value::Null)
}

fn deferred(reach: &Reach<'_>) -> Value {
    let Some(deferred) = &reach.workspace.services.deferred else { return Value::Null };
    let listed: Vec<Value> = deferred
        .specs()
        .iter()
        .map(|spec| {
            json!({
                "name": spec.name,
                "description": spec.description,
                "parameters": spec.schema,
                "loaded": deferred.is_loaded(&spec.name),
            })
        })
        .collect();
    Value::Array(listed)
}

fn load(reach: &Reach<'_>, arguments: &Value) -> Result<Value, String> {
    let deferred = reach.workspace.services.deferred.as_ref().ok_or("Every tool is already loaded.")?;
    let names = arguments.get("names").and_then(Value::as_array).ok_or("names must be a list of tool names")?;
    for name in names.iter().filter_map(Value::as_str) {
        deferred.load(name);
    }
    Ok(json!({ "waiting": deferred.waiting() }))
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::super::Fence;
    use super::*;
    use crate::model::ToolSpec;
    use crate::tools::Workspace;

    const SCHEDULES: [crate::tools::Effect; 1] = [crate::tools::Effect::Spawn(crate::tools::SpawnKind::Schedule)];

    fn reach_of(workspace: &Workspace) -> Reach<'_> {
        Reach { workspace, fence: Fence::of(&SCHEDULES), streams: None }
    }

    #[test]
    fn the_session_primitives_reach_the_peers_the_inbox_and_the_schedule() {
        peers::register(9501, Path::new("/work/api"));
        peers::register(9502, Path::new("/work/web"));
        let mut workspace = Workspace::new("/work/api");
        workspace.credentials.client = 9501;
        let reach = reach_of(&workspace);
        let listed = call(&reach, "peers", &Value::Null).expect("peers");
        let mine: Vec<&Value> = listed.as_array().into_iter().flatten().filter(|peer| peer["me"] == true).collect();
        assert_eq!(mine.len(), 1);
        assert_eq!(mine[0]["name"], "api-9501");
        call(&reach, "inbox.push", &json!({ "to": 9502, "source": "session api-9501", "text": "hi" })).expect("push");
        call(&reach, "inbox.push", &json!({ "to": 9502, "source": "gone", "text": "x" })).expect("push");
        call(&reach, "inbox.withdraw", &json!({ "to": 9502, "source": "gone" })).expect("withdraw");
        assert_eq!(inbox::take(9502), [Notice::new("session api-9501", "hi")]);
        let scheduled = call(&reach, "schedule", &json!({ "prompt": "wake", "after": 90 })).expect("schedule");
        assert!(scheduled["id"].as_str().is_some_and(|id| id.starts_with("wakeup_")), "{scheduled}");
        assert_eq!(schedules::list(9501).len(), 1);
        assert!(call(&reach, "schedule", &json!({ "prompt": "wake" })).is_err());
        let fenced = Reach { workspace: &workspace, fence: Fence::of(&[]), streams: None };
        assert!(
            call(&fenced, "schedule", &json!({ "prompt": "wake", "after": 90 })).is_err_and(|e| e.contains("Schedule"))
        );
        schedules::forget_client(9501);
        for client in [9501, 9502] {
            peers::forget(client);
        }
        assert!(call(&reach, "teleport", &Value::Null).is_err());
    }

    #[test]
    fn the_deferred_tools_are_listed_and_loaded_by_name() {
        let mut workspace = Workspace::new(".");
        assert_eq!(call(&reach_of(&workspace), "deferred", &Value::Null), Ok(Value::Null));
        let spec = |name: String| ToolSpec { name, description: String::new(), schema: json!({ "type": "object" }) };
        let specs = (0..25).map(|n| spec(format!("mcp__jira__tool{n}"))).collect();
        workspace.services.deferred = crate::tools::tool_search::defer(specs).1.map(std::sync::Arc::new);
        let reach = reach_of(&workspace);
        let listed = call(&reach, "deferred", &Value::Null).expect("deferred");
        assert_eq!(listed.as_array().map(Vec::len), Some(25));
        let loaded = call(&reach, "load", &json!({ "names": ["mcp__jira__tool3", "nothing"] })).expect("load");
        assert_eq!(loaded["waiting"].as_array().map(Vec::len), Some(24));
        assert_eq!(call(&reach, "deferred", &Value::Null).expect("deferred")[3]["loaded"], true);
    }
}
