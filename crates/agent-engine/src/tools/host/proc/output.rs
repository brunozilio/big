use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use domain::output::{Saving, clean, filter, program};
use serde_json::{Value, json};

use super::super::{Reach, text};
use crate::tools::Workspace;
use crate::tools::builtin::shell::savings;

pub(super) fn call(reach: &Reach<'_>, name: &str, arguments: &Value) -> Result<Value, String> {
    match name {
        "policy" => Ok(policy(reach.workspace)),
        "clean" => Ok(Value::String(clean(text(arguments, "text")?))),
        "filter" => filtered(arguments),
        "keep" => Ok(kept_whole(text(arguments, "text")?, reach.workspace)
            .map_or(Value::Null, |path| Value::String(path.display().to_string()))),
        "saving" => saving(reach.workspace, arguments),
        other => Err(format!("ctx.proc.{other} is not a function")),
    }
}

fn policy(workspace: &Workspace) -> Value {
    let shaping = workspace.policy.output;
    json!({
        "cleanup": shaping.cleanup,
        "filters": shaping.filters,
        "savings": shaping.savings,
        "limit": workspace.policy.bash_output.characters(),
    })
}

fn filtered(arguments: &Value) -> Result<Value, String> {
    let found = filter(text(arguments, "command")?, text(arguments, "text")?);
    Ok(found.map_or(Value::Null, |found| json!({ "text": found.text, "hidden": found.hidden, "by": found.by })))
}

fn saving(workspace: &Workspace, arguments: &Value) -> Result<Value, String> {
    let count = |key: &str| {
        arguments
            .get(key)
            .and_then(Value::as_u64)
            .and_then(|count| usize::try_from(count).ok())
            .ok_or_else(|| format!("{key} must be a count of characters"))
    };
    let saving =
        Saving { program: program(text(arguments, "command")?), before: count("before")?, after: count("after")? };
    savings::record(workspace.credentials.client, saving);
    Ok(Value::Null)
}

fn kept_whole(text: &str, workspace: &Workspace) -> Option<PathBuf> {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    let dir = crate::tools::persisted::results_dir(&workspace.session_id);
    let name = format!("bash-{}-{}.txt", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed));
    let path = dir.join(name);
    match crate::tools::persisted::write_private(&dir, &path, &workspace.policy.redacted(text)) {
        Ok(()) => Some(path),
        Err(error) => {
            tracing::warn!(%error, "could not keep a command's whole output");
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn a_command_s_whole_output_is_kept_for_the_owner_only() {
        use std::os::unix::fs::PermissionsExt;
        let mut workspace = Workspace::new(std::env::temp_dir());
        workspace.session_id = "shaped-92004".to_owned();
        let path = kept_whole("secret output", &workspace).expect("kept");
        let mode = |path: &std::path::Path| std::fs::metadata(path).expect("metadata").permissions().mode() & 0o777;
        let modes = (mode(path.parent().expect("folder")), mode(&path));
        let _ = std::fs::remove_dir_all(path.parent().expect("folder"));
        assert_eq!(modes, (0o700, 0o600));
    }
}
