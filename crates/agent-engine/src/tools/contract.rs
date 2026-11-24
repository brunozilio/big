use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use serde_json::Value;

use super::{Declared, Effect, Outcome, SpawnKind, Workspace};

const QUICK: Duration = Duration::from_secs(5);

struct Quiet;

impl script::Host for Quiet {
    fn call(&self, function: &str, _arguments: Value) -> Result<Value, String> {
        Err(format!("ctx.{function} is out of reach here: only execute calls the app"))
    }
}

pub fn quick(path: &str, source: &str, export: &str, arguments: &[Value], data: &Value) -> Result<Value, String> {
    let stop = Arc::new(AtomicBool::new(false));
    script::run_within(path, source, export, arguments, data, Arc::new(Quiet), stop, Some(QUICK))
}

pub fn effects(value: &Value, workspace: &Workspace) -> Result<Vec<Effect>, String> {
    match value {
        Value::Null => Ok(Vec::new()),
        Value::Array(list) => list.iter().filter_map(|entry| effect(entry, workspace).transpose()).collect(),
        other => Err(format!("effects must be a list, not {other}")),
    }
}

fn spawned(kind: &str) -> Result<SpawnKind, String> {
    match kind {
        "agent" => Ok(SpawnKind::Agent),
        "backgroundShell" => Ok(SpawnKind::BackgroundShell),
        "schedule" => Ok(SpawnKind::Schedule),
        "watch" => Ok(SpawnKind::Watch),
        other => Err(format!("{other} is not something a tool spawns")),
    }
}

fn install_for(file: &str, workspace: &Workspace) -> Option<Effect> {
    let lsp = workspace.services.lsp.as_ref()?;
    let file = workspace.resolve(file);
    let what = lsp.pending_install(&file)?;
    let server = domain::lsp::server_for(&file, &lsp.specs).map(|(spec, _)| spec.name.clone()).unwrap_or_default();
    Some(Effect::Install { server, what })
}

fn effect(entry: &Value, workspace: &Workspace) -> Result<Option<Effect>, String> {
    let fields = entry.as_object().filter(|fields| fields.len() == 1);
    let (key, value) = fields
        .and_then(|fields| fields.iter().next())
        .ok_or_else(|| format!("an effect has exactly one key, like {{ read: path }}: {entry}"))?;
    let text = || value.as_str().map(str::to_owned).ok_or_else(|| format!("{key} takes a string"));
    let field = |name: &str| {
        value
            .get(name)
            .and_then(Value::as_str)
            .map(str::to_owned)
            .ok_or_else(|| format!("{key}.{name} must be a string"))
    };
    Ok(Some(match key.as_str() {
        "installFor" => return Ok(install_for(&text()?, workspace)),
        "read" => Effect::ReadPath(workspace.place(&text()?)),
        "readTree" => Effect::ReadTree(workspace.place(&text()?)),
        "write" => Effect::WritePath(workspace.place(&text()?)),
        "edits" => Effect::Declared(Declared::Edits),
        "executes" => Effect::Declared(Declared::Executes),
        "exec" => Effect::exec(&text()?),
        "net" => Effect::Net { host: value.as_str().map(str::to_owned) },
        "fetch" => Effect::Fetch { host: value.as_str().map(str::to_owned) },
        "spawn" => Effect::Spawn(spawned(&text()?)?),
        "cancel" => Effect::Cancel { own: cancels_its_own(value, workspace) },
        "session" => Effect::Session { to: text()? },
        "resource" => Effect::Resource { server: field("server")?, uri: field("uri")? },
        "remote" => Effect::Remote { server: text()? },
        "install" => Effect::Install { server: field("server")?, what: field("what")? },
        other => return Err(format!("{other} is not an effect")),
    }))
}

fn cancels_its_own(value: &Value, workspace: &Workspace) -> bool {
    match value.get("shell").and_then(Value::as_str) {
        Some(id) => crate::tools::builtin::shell::shells::started_by(workspace.credentials.client, id),
        None => value.get("own").and_then(Value::as_bool).unwrap_or(false),
    }
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct Answer {
    content: Value,
    #[serde(default)]
    is_error: bool,
    #[serde(default)]
    images: Vec<domain::Image>,
    #[serde(default)]
    shown: Option<String>,
    #[serde(default)]
    edited: Option<domain::changes::Edited>,
    #[serde(default)]
    background: Option<domain::events::BackgroundChange>,
}

fn text_of(value: Value) -> String {
    match value {
        Value::String(text) => text,
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

pub fn outcome(value: Value) -> Outcome {
    let answered = value.as_object().is_some_and(|fields| fields.contains_key("content"));
    if !answered {
        return Outcome::ok(text_of(value));
    }
    match serde_json::from_value::<Answer>(value) {
        Ok(answer) => Outcome {
            content: text_of(answer.content),
            is_error: answer.is_error,
            images: answer.images,
            shown: answer.shown,
            edited: answer.edited.map(Box::new),
            background: answer.background.map(Box::new),
        },
        Err(error) => Outcome::error(format!("the tool answered in a shape that does not read: {error}")),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn effects_read_as_the_permission_engine_s_and_a_bad_one_says_why() {
        let dir = tempfile::tempdir().expect("dir");
        let workspace = Workspace::new(dir.path());
        let read = effects(
            &json!([{ "read": "a.txt" }, { "exec": "ls" }, { "spawn": "agent" }, { "net": null }, { "edits": true }]),
            &workspace,
        )
        .expect("effects");
        assert_eq!(read.len(), 5);
        assert!(matches!(&read[0], Effect::ReadPath(place) if place.written.ends_with("a.txt")));
        assert!(matches!(read[1], Effect::Exec(_)));
        assert_eq!(read[2], Effect::Spawn(SpawnKind::Agent));
        assert_eq!(read[3], Effect::Net { host: None });
        assert_eq!(read[4], Effect::Declared(Declared::Edits));
        assert_eq!(effects(&Value::Null, &workspace), Ok(Vec::new()));
        assert!(effects(&json!([{ "teleport": 1 }]), &workspace).is_err());
        assert!(effects(&json!([{ "read": "a", "write": "b" }]), &workspace).is_err());
        assert!(effects(&json!("read"), &workspace).is_err());
    }

    #[test]
    fn an_answer_keeps_everything_a_tool_gives_back() {
        assert_eq!(outcome(json!("plain")), Outcome::ok("plain"));
        assert_eq!(outcome(Value::Null), Outcome::ok(""));
        assert_eq!(outcome(json!({ "n": 1 })), Outcome::ok(r#"{"n":1}"#));
        let full = outcome(json!({ "content": "done", "shown": "3 files", "background": { "Started": "b1" } }));
        assert_eq!(
            full,
            Outcome::ok("done")
                .showing("3 files")
                .in_the_background(domain::events::BackgroundChange::Started("b1".into()))
        );
        assert_eq!(outcome(json!({ "content": "no", "isError": true })), Outcome::error("no"));
        assert!(outcome(json!({ "content": "x", "images": "bad" })).is_error);
    }

    #[test]
    fn a_quick_export_runs_without_the_app() {
        let source = "export function effects(input: { path: string }) { return [{ read: input.path }]; }\nexport function summary(input) { return ctx_less(input); }\nfunction ctx_less(input) { return input.path; }\nexport function reach(input, ctx) { return ctx.readFile(input.path); }";
        let input = json!({ "path": "a" });
        assert_eq!(
            quick("tools/X.ts", source, "effects", std::slice::from_ref(&input), &Value::Null),
            Ok(json!([{ "read": "a" }]))
        );
        assert_eq!(quick("tools/X.ts", source, "summary", std::slice::from_ref(&input), &Value::Null), Ok(json!("a")));
        assert!(
            quick("tools/X.ts", source, "reach", &[input], &Value::Null)
                .is_err_and(|error| error.contains("out of reach"))
        );
    }
}
