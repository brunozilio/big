use std::path::PathBuf;

use serde_json::{Value, json};

use super::{Reach, text};
use crate::tools::read_files::Freshness;
use crate::tools::shown;

pub const SPACE: &str = "files";

pub fn call(reach: &Reach<'_>, name: &str, arguments: &Value) -> Result<Value, String> {
    match name {
        "place" => place(reach, arguments),
        "raw" => Ok(Value::Bool(crate::tools::raw_text::active())),
        "freshness" => freshness(reach, arguments),
        "markRead" => mark_read(reach, arguments),
        "seen" => seen(reach, arguments),
        "saw" => saw(reach, arguments),
        "anchored" => anchored(reach, arguments),
        "checkpoint" => checkpoint(reach, arguments),
        "wrote" => wrote(reach, arguments),
        other => Err(format!("ctx.{SPACE}.{other} is not a function")),
    }
}

fn readable(reach: &Reach<'_>, arguments: &Value) -> Result<PathBuf, String> {
    let path = text(arguments, "path")?;
    reach.fence.reads(&reach.workspace.place(path).real)?;
    Ok(reach.workspace.resolve(path))
}

fn writable(reach: &Reach<'_>, arguments: &Value) -> Result<PathBuf, String> {
    let path = text(arguments, "path")?;
    reach.fence.writes(&reach.workspace.place(path).real)?;
    Ok(reach.workspace.resolve(path))
}

fn line(arguments: &Value, key: &str) -> Result<usize, String> {
    let number = arguments.get(key).and_then(Value::as_u64).ok_or_else(|| format!("{key} must be a line number"))?;
    usize::try_from(number).map_err(|error| error.to_string())
}

fn place(reach: &Reach<'_>, arguments: &Value) -> Result<Value, String> {
    let path = reach.workspace.resolve(text(arguments, "path")?);
    Ok(json!({ "shown": shown(&path, reach.workspace), "full": path.display().to_string() }))
}

fn freshness(reach: &Reach<'_>, arguments: &Value) -> Result<Value, String> {
    let path = readable(reach, arguments)?;
    let said = match reach.workspace.freshness(&path) {
        Freshness::Unread => "unread",
        Freshness::Changed => "changed",
        Freshness::Partial => "partial",
        Freshness::Current => "current",
    };
    Ok(json!(said))
}

fn mark_read(reach: &Reach<'_>, arguments: &Value) -> Result<Value, String> {
    let path = readable(reach, arguments)?;
    if arguments.get("whole").and_then(Value::as_bool).unwrap_or(true) {
        reach.workspace.mark_read(&path);
    } else {
        reach.workspace.mark_partly_read(&path);
    }
    Ok(Value::Null)
}

fn seen(reach: &Reach<'_>, arguments: &Value) -> Result<Value, String> {
    let path = readable(reach, arguments)?;
    let (first, last) = (line(arguments, "first")?, line(arguments, "last")?);
    Ok(Value::Bool(reach.workspace.tracking.seen_files.covers(&path, first, last)))
}

fn saw(reach: &Reach<'_>, arguments: &Value) -> Result<Value, String> {
    let path = readable(reach, arguments)?;
    let (first, last) = (line(arguments, "first")?, line(arguments, "last")?);
    reach.workspace.tracking.seen_files.saw(&path, first, last);
    Ok(Value::Null)
}

fn anchored(reach: &Reach<'_>, arguments: &Value) -> Result<Value, String> {
    let path = readable(reach, arguments)?;
    Ok(Value::String(reach.workspace.anchored(&path)))
}

fn checkpoint(reach: &Reach<'_>, arguments: &Value) -> Result<Value, String> {
    let path = writable(reach, arguments)?;
    Ok(Value::String(reach.workspace.before_write(&path).unwrap_or_default()))
}

fn wrote(reach: &Reach<'_>, arguments: &Value) -> Result<Value, String> {
    let path = writable(reach, arguments)?;
    let before = arguments.get("before").and_then(Value::as_str).unwrap_or_default();
    let after = text(arguments, "after")?;
    let workspace = reach.workspace;
    workspace.mark_read(&path);
    let tests = crate::tools::builtin::shell::affected::start(workspace, &path);
    let others = workspace.claim(&path);
    let remembered = workspace.anchored(&path);
    let callers = workspace.callers(&path, before, after);
    let notes = [workspace.diagnosed(&path, after), callers, tests, others, remembered];
    Ok(Value::String(notes.concat()))
}

#[cfg(test)]
mod tests {
    use super::super::Fence;
    use super::*;
    use crate::tools::{Effect, Workspace};

    #[test]
    fn what_was_read_is_remembered_only_inside_the_fence() {
        let dir = tempfile::tempdir().expect("dir");
        std::fs::write(dir.path().join("a.txt"), "one\ntwo\n").expect("a");
        let workspace = Workspace::new(dir.path());
        let effects = [Effect::ReadPath(workspace.place("a.txt"))];
        let reach = Reach { workspace: &workspace, fence: Fence::of(&effects), streams: None };
        let a = json!({ "path": "a.txt" });
        assert_eq!(call(&reach, "place", &a).expect("place")["shown"], "a.txt");
        assert_eq!(call(&reach, "freshness", &a), Ok(json!("unread")));
        call(&reach, "markRead", &json!({ "path": "a.txt", "whole": false })).expect("mark");
        assert_eq!(call(&reach, "freshness", &a), Ok(json!("partial")));
        call(&reach, "saw", &json!({ "path": "a.txt", "first": 1, "last": 2 })).expect("saw");
        assert_eq!(call(&reach, "seen", &json!({ "path": "a.txt", "first": 2, "last": 2 })), Ok(json!(true)));
        assert!(call(&reach, "freshness", &json!({ "path": "b.txt" })).is_err());
        assert!(call(&reach, "wrote", &json!({ "path": "a.txt", "after": "x" })).is_err());
        assert_eq!(call(&reach, "raw", &Value::Null), Ok(json!(false)));
    }
}
