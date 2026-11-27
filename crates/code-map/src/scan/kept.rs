use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock, Mutex, PoisonError};
use std::time::{Duration, Instant, SystemTime};

use super::{DiskSource, FileStat, Source, refresh_from};
use crate::map::CodeMap;

pub const RECHECK: Duration = Duration::from_secs(10);

pub type Marks = Vec<Option<SystemTime>>;

struct Kept {
    map: Arc<CodeMap>,
    stats: Vec<FileStat>,
    marks: Marks,
    checked: Instant,
}

static KEPT: LazyLock<Mutex<HashMap<PathBuf, Kept>>> = LazyLock::new(Mutex::default);

pub fn marks(root: &Path) -> Marks {
    let git = root.ancestors().map(|folder| folder.join(".git")).find(|git| git.exists());
    let mut watched = vec![root.to_path_buf()];
    if let Some(git) = git {
        watched.extend([git.join("index"), git.join("HEAD"), git]);
    }
    watched.iter().map(|path| std::fs::metadata(path).and_then(|found| found.modified()).ok()).collect()
}

pub fn current(root: &Path, cache: &Path) -> Arc<CodeMap> {
    current_with(&DiskSource::new(root), marks(root), cache, Instant::now())
}

pub fn current_with(source: &dyn Source, marks: Marks, cache: &Path, now: Instant) -> Arc<CodeMap> {
    let recent = |kept: &Kept| kept.marks == marks && now.saturating_duration_since(kept.checked) < RECHECK;
    if let Some(kept) = KEPT.lock().unwrap_or_else(PoisonError::into_inner).get(cache).filter(|kept| recent(kept)) {
        return Arc::clone(&kept.map);
    }
    let stats = source.files();
    let mut known = KEPT.lock().unwrap_or_else(PoisonError::into_inner);
    if let Some(kept) = known.get_mut(cache).filter(|kept| kept.stats == stats) {
        kept.marks = marks;
        kept.checked = now;
        return Arc::clone(&kept.map);
    }
    drop(known);
    let map = Arc::new(refresh_from(source, cache, stats.clone()));
    let kept = Kept { map: Arc::clone(&map), stats, marks, checked: now };
    KEPT.lock().unwrap_or_else(PoisonError::into_inner).insert(cache.to_path_buf(), kept);
    map
}
