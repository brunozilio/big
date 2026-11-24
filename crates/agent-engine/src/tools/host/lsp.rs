use std::path::Path;
use std::time::Duration;

use domain::lsp::answer::RequestError;
use domain::lsp::uri::{path_of, uri_of};
use serde_json::{Value, json};

use super::{Reach, text};
use crate::lsp::{Installing, Lsp, Opened};
use crate::tools::shown;

pub const SPACE: &str = "lsp";

const ANSWER_WAIT: Duration = Duration::from_secs(60);

pub fn call(reach: &Reach<'_>, name: &str, arguments: &Value) -> Result<Value, String> {
    match name {
        "available" => Ok(Value::Bool(reach.workspace.services.lsp.is_some())),
        "pending" => pending(reach, arguments),
        "open" => open(reach, arguments),
        "request" => request(reach, arguments),
        "diagnostics" => diagnostics(reach, arguments),
        "published" => published(reach, arguments),
        "where" => located(reach, arguments),
        other => Err(format!("ctx.{SPACE}.{other} is not a function")),
    }
}

fn servers<'r>(reach: &'r Reach<'_>) -> Result<&'r Lsp, String> {
    reach.workspace.services.lsp.as_ref().ok_or_else(|| "No language servers are available in this run.".to_owned())
}

fn opened(reach: &Reach<'_>, file: &Path) -> Result<(Opened, Option<String>), String> {
    let workspace = reach.workspace;
    servers(reach)?.open(file, &workspace.cwd, &workspace.stop, Installing::Allowed)
}

fn pending(reach: &Reach<'_>, arguments: &Value) -> Result<Value, String> {
    let file = reach.workspace.resolve(text(arguments, "path")?);
    Ok(servers(reach)?.pending_install(&file).map_or(Value::Null, Value::String))
}

fn open(reach: &Reach<'_>, arguments: &Value) -> Result<Value, String> {
    let place = reach.workspace.place(text(arguments, "path")?);
    reach.fence.reads(&place.real)?;
    let file = reach.workspace.resolve(text(arguments, "path")?);
    let named = shown(&file, reach.workspace);
    servers(reach)?;
    let text = std::fs::read_to_string(&file).map_err(|error| format!("Cannot read {named}: {error}"))?;
    let (opened, installed) = opened(reach, &file)?;
    let version = opened.server.sync(&file, &opened.language, &text)?;
    Ok(json!({ "file": named, "uri": uri_of(&file), "text": text, "version": version, "installed": installed }))
}

fn request(reach: &Reach<'_>, arguments: &Value) -> Result<Value, String> {
    let file = reach.workspace.resolve(text(arguments, "path")?);
    let method = text(arguments, "method")?;
    let params = arguments.get("params").cloned().unwrap_or(Value::Null);
    let (opened, _) = opened(reach, &file)?;
    Ok(match opened.server.request(method, params, &reach.workspace.stop, ANSWER_WAIT) {
        Ok(answer) => json!({ "answer": answer }),
        Err(RequestError::Busy(text)) => json!({ "failed": text, "busy": true }),
        Err(RequestError::Failed(text)) => json!({ "failed": text, "busy": false }),
    })
}

fn diagnostics(reach: &Reach<'_>, arguments: &Value) -> Result<Value, String> {
    let file = reach.workspace.resolve(text(arguments, "path")?);
    let version = arguments.get("version").and_then(Value::as_i64).ok_or("version must be a number")?;
    let wait = Duration::from_secs(arguments.get("waitSeconds").and_then(Value::as_u64).unwrap_or(10));
    let (opened, _) = opened(reach, &file)?;
    Ok(opened.server.diagnostics(&file, version, wait).map_or(Value::Null, Value::from))
}

fn relative(path: &Path, cwd: &Path) -> String {
    let real = cwd.canonicalize().unwrap_or_else(|_| cwd.to_path_buf());
    let inside = path.strip_prefix(cwd).or_else(|_| path.strip_prefix(&real));
    inside.unwrap_or(path).display().to_string()
}

fn published(reach: &Reach<'_>, arguments: &Value) -> Result<Value, String> {
    let file = reach.workspace.resolve(text(arguments, "path")?);
    let (opened, _) = opened(reach, &file)?;
    let cwd = &reach.workspace.cwd;
    let files: Vec<Value> = opened
        .server
        .published()
        .into_iter()
        .filter_map(|(uri, items)| Some(json!({ "file": relative(&path_of(&uri)?, cwd), "items": items })))
        .collect();
    Ok(Value::Array(files))
}

fn located(reach: &Reach<'_>, arguments: &Value) -> Result<Value, String> {
    let Some(path) = path_of(text(arguments, "uri")?) else { return Ok(Value::Null) };
    let shown = relative(&path, &reach.workspace.cwd);
    let line = arguments.get("line").and_then(Value::as_u64).and_then(|line| usize::try_from(line).ok());
    let text = line.and_then(|line| std::fs::read_to_string(&path).ok()?.lines().nth(line).map(str::to_owned));
    Ok(json!({ "shown": shown, "text": text }))
}

#[cfg(test)]
mod tests {
    use super::super::Fence;
    use super::*;
    use crate::lsp::fake::ScriptedServer;
    use crate::tools::{Effect, Workspace};

    #[test]
    fn the_language_server_primitives_open_ask_and_locate() {
        let dir = tempfile::tempdir().expect("dir");
        std::fs::write(dir.path().join("main.rs"), "fn main() {}\nfn other() {}\n").expect("main");
        let server = ScriptedServer::answering([Ok(json!({ "contents": "fn main()" }))]);
        let mut workspace = Workspace::new(dir.path());
        assert_eq!(
            call(&Reach { workspace: &workspace, fence: Fence::of(&[]), streams: None }, "available", &Value::Null),
            Ok(json!(false))
        );
        workspace.services.lsp = Some(server.lsp());
        let effects = [Effect::ReadPath(workspace.place("main.rs"))];
        let reach = Reach { workspace: &workspace, fence: Fence::of(&effects), streams: None };
        assert_eq!(call(&reach, "pending", &json!({ "path": "main.rs" })), Ok(Value::Null));
        let opened = call(&reach, "open", &json!({ "path": "main.rs" })).expect("open");
        assert_eq!((opened["file"].as_str(), opened["version"].as_i64()), (Some("main.rs"), Some(1)));
        assert!(call(&reach, "open", &json!({ "path": "other.rs" })).is_err(), "outside the fence");
        let asked = json!({ "path": "main.rs", "method": "textDocument/hover", "params": {} });
        assert_eq!(call(&reach, "request", &asked), Ok(json!({ "answer": { "contents": "fn main()" } })));
        let uri = opened["uri"].clone();
        let found = call(&reach, "where", &json!({ "uri": uri, "line": 1 })).expect("where");
        assert_eq!(found, json!({ "shown": "main.rs", "text": "fn other() {}" }));
        assert_eq!(call(&reach, "where", &json!({ "uri": "https://x.dev/a" })), Ok(Value::Null));
        assert_eq!(call(&reach, "diagnostics", &json!({ "path": "main.rs", "version": 1 })), Ok(Value::Null));
        assert_eq!(call(&reach, "published", &json!({ "path": "main.rs" })), Ok(json!([])));
    }
}
