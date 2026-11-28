use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{Value, json};

use super::{Reach, text};
use crate::tools::shown;

pub const SPACE: &str = "fs";

const MOST_READ: u64 = 64 << 20;

pub fn call(reach: &Reach<'_>, name: &str, arguments: &Value) -> Result<Value, String> {
    match name {
        "read" => read(reach, arguments),
        "stat" => stat(reach, arguments),
        "write" => write(reach, arguments),
        "walk" => walk(reach, arguments),
        other => Err(format!("ctx.fs.{other} is not a function")),
    }
}

fn millis(time: SystemTime) -> u64 {
    time.duration_since(UNIX_EPOCH).map_or(0, |since| u64::try_from(since.as_millis()).unwrap_or(u64::MAX))
}

fn read(reach: &Reach<'_>, arguments: &Value) -> Result<Value, String> {
    let place = reach.workspace.place(text(arguments, "path")?);
    reach.fence.reads(&place.real)?;
    let size = std::fs::metadata(&place.real).map_err(|error| error.to_string())?.len();
    if size > MOST_READ {
        return Err(format!("the file has {size} bytes, more than the {MOST_READ} a read takes"));
    }
    let bytes = std::fs::read(&place.real).map_err(|error| error.to_string())?;
    match String::from_utf8(bytes) {
        Ok(text) => Ok(json!({ "text": text })),
        Err(_) => Ok(json!({ "binary": true, "bytes": size })),
    }
}

fn stat(reach: &Reach<'_>, arguments: &Value) -> Result<Value, String> {
    let place = reach.workspace.place(text(arguments, "path")?);
    reach.fence.reads(&place.real)?;
    match std::fs::metadata(&place.real) {
        Ok(meta) => Ok(json!({
            "exists": true,
            "dir": meta.is_dir(),
            "bytes": meta.len(),
            "modified": meta.modified().map(millis).unwrap_or_default(),
        })),
        Err(_) => Ok(json!({ "exists": false })),
    }
}

fn write(reach: &Reach<'_>, arguments: &Value) -> Result<Value, String> {
    let place = reach.workspace.place(text(arguments, "path")?);
    reach.fence.writes(&place.real)?;
    let body = text(arguments, "text")?;
    if let Some(parent) = place.real.parent() {
        platform::confine::dirs_like_ancestor(parent)
            .map_err(|error| format!("Could not create {}: {error}", parent.display()))?;
    }
    platform::atomic::write(&place.real, body.as_bytes()).map_err(|error| error.to_string())?;
    Ok(Value::Null)
}

fn walk(reach: &Reach<'_>, arguments: &Value) -> Result<Value, String> {
    let root = arguments.get("root").and_then(Value::as_str).unwrap_or(".");
    let place = reach.workspace.place(root);
    reach.fence.reads(&place.real)?;
    let pattern = arguments.get("glob").and_then(Value::as_str).unwrap_or("**/*");
    let matcher = globset::GlobBuilder::new(pattern)
        .literal_separator(true)
        .build()
        .map_err(|error| format!("Invalid pattern: {error}"))?
        .compile_matcher();
    let base = reach.workspace.resolve(root);
    let found: Vec<Value> = ignore::WalkBuilder::new(&base)
        .hidden(false)
        .filter_entry(|entry| entry.file_name() != ".git")
        .build()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_some_and(|kind| kind.is_file()))
        .filter(|entry| entry.path().strip_prefix(&base).is_ok_and(|relative| matcher.is_match(relative)))
        .map(|entry| {
            let modified = entry.metadata().ok().and_then(|meta| meta.modified().ok()).map(millis).unwrap_or_default();
            json!({ "path": shown(entry.path(), reach.workspace), "modified": modified })
        })
        .collect();
    Ok(Value::Array(found))
}

#[cfg(test)]
mod tests {
    use super::super::Fence;
    use super::*;
    use crate::tools::{Effect, Workspace};

    #[test]
    fn the_files_primitives_stay_inside_the_fence() {
        let dir = tempfile::tempdir().expect("dir");
        let root = dir.path();
        std::fs::create_dir_all(root.join("src")).expect("src");
        std::fs::write(root.join("src/a.rs"), "fn a() {}").expect("a");
        std::fs::write(root.join("secret"), "s").expect("secret");
        let workspace = Workspace::new(root);
        let effects = [Effect::ReadTree(workspace.place("src")), Effect::WritePath(workspace.place("out/b.txt"))];
        let reach = Reach { workspace: &workspace, fence: Fence::of(&effects), streams: None };
        assert_eq!(call(&reach, "read", &json!({ "path": "src/a.rs" })), Ok(json!({ "text": "fn a() {}" })));
        assert!(call(&reach, "read", &json!({ "path": "secret" })).is_err());
        assert_eq!(call(&reach, "write", &json!({ "path": "out/b.txt", "text": "b" })), Ok(Value::Null));
        assert_eq!(std::fs::read_to_string(root.join("out/b.txt")).expect("written"), "b");
        assert!(call(&reach, "write", &json!({ "path": "src/a.rs", "text": "x" })).is_err());
        let walked = call(&reach, "walk", &json!({ "root": "src", "glob": "**/*.rs" })).expect("walk");
        assert_eq!(walked[0]["path"].as_str().map(|path| path.replace('\\', "/")), Some("src/a.rs".to_owned()));
        assert_eq!(call(&reach, "stat", &json!({ "path": "src/a.rs" })).expect("stat")["exists"], true);
    }
}
