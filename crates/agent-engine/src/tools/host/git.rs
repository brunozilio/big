mod changes;
mod command;

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};

use platform::sync::Locked;
use serde_json::{Value, json};

use super::Reach;
use crate::tools::Workspace;

pub const SPACE: &str = "git";

const HELD_AT_MOST: usize = 16;

struct Held {
    client: u64,
    watched: changes::Watched,
}

fn held() -> &'static Mutex<BTreeMap<u64, Held>> {
    static HELD: OnceLock<Mutex<BTreeMap<u64, Held>>> = OnceLock::new();
    HELD.get_or_init(Mutex::default)
}

pub fn call(reach: &Reach<'_>, name: &str, arguments: &Value) -> Result<Value, String> {
    match name {
        "snapshot" => snapshot(reach),
        "since" => since(reach.workspace, snapshot_id(arguments)?).map(|skipped| json!(skipped)),
        "forget" => {
            taken(reach.workspace.credentials.client, snapshot_id(arguments)?);
            Ok(Value::Null)
        }
        other => Err(format!("ctx.{SPACE}.{other} is not a function")),
    }
}

fn snapshot_id(arguments: &Value) -> Result<u64, String> {
    arguments.get("id").and_then(Value::as_u64).ok_or_else(|| "id must be a snapshot's id".to_owned())
}

fn snapshot(reach: &Reach<'_>) -> Result<Value, String> {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    reach.fence.runs_any()?;
    let Some(watched) = changes::before(reach.workspace) else { return Ok(Value::Null) };
    let id = NEXT.fetch_add(1, Ordering::Relaxed);
    let mut all = held().locked();
    all.insert(id, Held { client: reach.workspace.credentials.client, watched });
    while all.len() > HELD_AT_MOST {
        all.pop_first();
    }
    Ok(json!({ "id": id }))
}

fn taken(client: u64, id: u64) -> Option<changes::Watched> {
    let mut all = held().locked();
    if !all.get(&id).is_some_and(|held| held.client == client) {
        return None;
    }
    all.remove(&id).map(|held| held.watched)
}

pub(super) fn since(workspace: &Workspace, id: u64) -> Result<Vec<String>, String> {
    let watched = taken(workspace.credentials.client, id).ok_or_else(|| format!("no snapshot {id}"))?;
    let skipped: Vec<PathBuf> = watched.after(workspace);
    Ok(skipped.iter().map(|path| path.display().to_string()).collect())
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::super::Fence;
    use super::*;
    use crate::tools::Effect;

    #[test]
    fn a_snapshot_is_taken_only_by_a_call_that_runs_a_command_and_read_back_once_by_its_owner() {
        let dir = tempfile::tempdir().expect("dir");
        let ran = std::process::Command::new("git").args(["init", "-q"]).current_dir(dir.path()).status();
        assert!(ran.expect("git").success());
        let mut workspace = Workspace::new(dir.path());
        workspace.services.files = Some(Arc::default());
        workspace.credentials.client = 77_001;
        let quiet = Reach { workspace: &workspace, fence: Fence::of(&[]), streams: None };
        assert!(call(&quiet, "snapshot", &Value::Null).is_err());
        let effects = [Effect::exec("touch made.txt")];
        let reach = Reach { workspace: &workspace, fence: Fence::of(&effects), streams: None };
        let id = call(&reach, "snapshot", &Value::Null).expect("snapshot")["id"].clone();
        let mut other = Workspace::new(dir.path());
        other.credentials.client = 77_002;
        assert!(since(&other, id.as_u64().unwrap_or_default()).is_err(), "another client cannot read it");
        std::fs::write(dir.path().join("made.txt"), "").expect("made");
        assert_eq!(call(&reach, "since", &json!({ "id": id })), Ok(json!([])));
        assert!(call(&reach, "since", &json!({ "id": id })).is_err(), "it is read once");
        let files = workspace.services.files.as_ref().expect("files").take().files;
        assert!(files.keys().any(|path| path.ends_with("made.txt")), "{files:?}");
    }
}
