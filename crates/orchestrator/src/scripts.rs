use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use agent_engine::tools::{bank, bank_run};
use domain::workflow::WorkItem;
use serde_json::Value;

use crate::agent::{Failure, blocking};
use crate::protocol::Command;
use crate::shared::Shared;

const FINGERPRINTS: &str = "fingerprints";
const SCRATCH: &str = "scratch";

struct Stopping(Arc<AtomicBool>);

impl Drop for Stopping {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Relaxed);
    }
}

pub fn fingerprinted(data: &Path, name: &str, input: &Value) -> Result<PathBuf, Failure> {
    let source = bank::named(name).and_then(|script| script.source()).map_err(Failure::permanent)?;
    let print = domain::effect::fingerprint(&source, input);
    Ok(data.join(FINGERPRINTS).join(format!("{print}.json")))
}

pub fn reused(kept: &Path) -> Option<Value> {
    let text = std::fs::read_to_string(kept).ok()?;
    serde_json::from_str(&text).ok()
}

pub fn remember(kept: &Path, value: &Value) {
    let written =
        kept.parent().map_or(Ok(()), std::fs::create_dir_all).and_then(|()| std::fs::write(kept, value.to_string()));
    if let Err(error) = written {
        tracing::warn!(%error, "a pure script's result could not be kept for reuse");
    }
}

async fn folder(shared: &Shared, item: &WorkItem, command: &Command) -> Result<PathBuf, Failure> {
    if !shared.checkouts(item).is_empty() {
        return crate::effects::worktree_of(shared, item, command).await;
    }
    let scratch = shared.config.data.join(SCRATCH).join(item.id.to_string());
    std::fs::create_dir_all(&scratch).map_err(|error| Failure::permanent(error.to_string()))?;
    Ok(scratch)
}

pub async fn run(shared: &Shared, item: &WorkItem, command: &Command) -> Result<Value, Failure> {
    let name = command.text("name");
    let input = command.args["input"].clone();
    let kept = match command.args["pure"].as_bool() {
        Some(true) => Some(fingerprinted(&shared.config.data, &name, &input)?),
        _ => None,
    };
    if let Some(value) = kept.as_deref().and_then(reused) {
        tracing::info!(script = name, "a pure script's result was reused by its fingerprint");
        return Ok(value);
    }
    let cwd = folder(shared, item, command).await?;
    let stop = Arc::new(AtomicBool::new(false));
    let _stopping = Stopping(Arc::clone(&stop));
    let value = blocking(move || bank_run::run(&name, &input, &cwd, stop)).await?.map_err(Failure::permanent)?;
    if let Some(kept) = &kept {
        remember(kept, &value);
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn a_pure_result_is_found_again_by_the_same_script_and_input_only() {
        let data = tempfile::tempdir().expect("data");
        let first = fingerprinted(data.path(), "count-matches", &json!({ "pattern": "TODO" })).expect("print");
        assert_eq!(reused(&first), None);
        remember(&first, &json!({ "total": 3 }));
        let again = fingerprinted(data.path(), "count-matches", &json!({ "pattern": "TODO" })).expect("print");
        assert_eq!(reused(&again), Some(json!({ "total": 3 })));
        let other = fingerprinted(data.path(), "count-matches", &json!({ "pattern": "FIXME" })).expect("print");
        assert_eq!(reused(&other), None);
        assert!(fingerprinted(data.path(), "nothing-like-this", &json!({})).is_err());
    }
}
