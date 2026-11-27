use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, SystemTime};

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Keeping {
    pub age: Duration,
    pub bytes: u64,
}

impl Keeping {
    pub fn from_config() -> Keeping {
        let number = |text: &str, fallback: u64| text.trim().parse::<u64>().unwrap_or(fallback);
        Keeping {
            age: Duration::from_secs(number(brand::workflow::FINGERPRINT_DAYS, 30).saturating_mul(86_400)),
            bytes: number(brand::workflow::FINGERPRINT_MAX_MB, 64).saturating_mul(1 << 20),
        }
    }

    fn expired(&self, written: SystemTime, now: SystemTime) -> bool {
        now.duration_since(written).is_ok_and(|age| age > self.age)
    }
}

fn written(path: &Path) -> Option<(SystemTime, u64)> {
    let metadata = std::fs::metadata(path).ok()?;
    Some((metadata.modified().ok()?, metadata.len()))
}

fn forget(path: &Path) {
    if let Err(error) = std::fs::remove_file(path) {
        tracing::warn!(%error, path = %path.display(), "a kept script result could not be removed");
    }
}

pub fn reused(kept: &Path, keeping: Keeping, now: SystemTime) -> Option<Value> {
    let (at, _) = written(kept)?;
    if keeping.expired(at, now) {
        forget(kept);
        return None;
    }
    let text = std::fs::read_to_string(kept).ok()?;
    serde_json::from_str(&text).ok()
}

pub fn evict(folder: &Path, keeping: Keeping, now: SystemTime) {
    let Ok(entries) = std::fs::read_dir(folder) else { return };
    let mut kept: Vec<(SystemTime, u64, PathBuf)> = Vec::new();
    for path in entries.flatten().map(|entry| entry.path()) {
        match written(&path) {
            Some((at, _)) if keeping.expired(at, now) => forget(&path),
            Some((at, size)) => kept.push((at, size, path)),
            None => {}
        }
    }
    kept.sort();
    let mut total: u64 = kept.iter().map(|(_, size, _)| size).sum();
    for (_, size, path) in kept {
        if total <= keeping.bytes {
            break;
        }
        forget(&path);
        total = total.saturating_sub(size);
    }
}

pub fn remember(kept: &Path, value: &Value, keeping: Keeping) {
    let written =
        kept.parent().map_or(Ok(()), std::fs::create_dir_all).and_then(|()| std::fs::write(kept, value.to_string()));
    if let Err(error) = written {
        tracing::warn!(%error, "a pure script's result could not be kept for reuse");
    }
    if let Some(folder) = kept.parent() {
        evict(folder, keeping, SystemTime::now());
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
    let keeping = Keeping::from_config();
    if let Some(value) = kept.as_deref().and_then(|kept| reused(kept, keeping, SystemTime::now())) {
        tracing::info!(script = name, "a pure script's result was reused by its fingerprint");
        return Ok(value);
    }
    let cwd = folder(shared, item, command).await?;
    let stop = Arc::new(AtomicBool::new(false));
    let _stopping = Stopping(Arc::clone(&stop));
    let value = blocking(move || bank_run::run(&name, &input, &cwd, stop)).await?.map_err(Failure::permanent)?;
    if let Some(kept) = &kept {
        remember(kept, &value, keeping);
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    const KEEPING: Keeping = Keeping { age: Duration::from_secs(3600), bytes: 1 << 20 };

    #[test]
    fn a_pure_result_is_found_again_by_the_same_script_and_input_only() {
        let data = tempfile::tempdir().expect("data");
        let now = SystemTime::now();
        let first = fingerprinted(data.path(), "count-matches", &json!({ "pattern": "TODO" })).expect("print");
        assert_eq!(reused(&first, KEEPING, now), None);
        remember(&first, &json!({ "total": 3 }), KEEPING);
        let again = fingerprinted(data.path(), "count-matches", &json!({ "pattern": "TODO" })).expect("print");
        assert_eq!(reused(&again, KEEPING, now), Some(json!({ "total": 3 })));
        let other = fingerprinted(data.path(), "count-matches", &json!({ "pattern": "FIXME" })).expect("print");
        assert_eq!(reused(&other, KEEPING, now), None);
        assert!(fingerprinted(data.path(), "nothing-like-this", &json!({})).is_err());
        let later = now + Duration::from_secs(7200);
        assert_eq!(reused(&again, KEEPING, later), None, "expired");
        assert!(!again.exists(), "an expired result is removed");
    }

    #[test]
    fn the_kept_results_stay_under_the_size_limit_dropping_the_oldest_first() {
        let folder = tempfile::tempdir().expect("folder");
        let small = Keeping { age: Duration::from_secs(3600), bytes: 25 };
        let base = SystemTime::now() - Duration::from_secs(60);
        for (at, name) in ["a", "b", "c"].iter().enumerate() {
            let path = folder.path().join(format!("{name}.json"));
            std::fs::write(&path, "0123456789").expect("write");
            let file = std::fs::File::options().write(true).open(&path).expect("open");
            file.set_modified(base + Duration::from_secs(at as u64)).expect("time");
        }
        evict(folder.path(), small, SystemTime::now());
        let left: Vec<bool> = ["a", "b", "c"].iter().map(|name| folder.path().join(format!("{name}.json")).exists()).collect();
        assert_eq!(left, [false, true, true]);
        evict(folder.path(), Keeping { age: Duration::from_secs(1), bytes: 1 << 20 }, SystemTime::now());
        assert!(!folder.path().join("c.json").exists(), "older than the age limit");
    }
}
