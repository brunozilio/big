use std::sync::{Mutex, PoisonError};

use domain::label::{Levels, Mode};

pub const FILE: &str = "safety/labels.ts";
const EXPORT: &str = "labels";

static LEVELS: Mutex<Option<(String, Levels)>> = Mutex::new(None);

pub fn levels() -> Levels {
    let folder = content::folder();
    let source = content::user_text(folder.as_deref(), FILE).or_else(|| content::builtin_text(FILE));
    source.map(|source| levels_of(&source)).unwrap_or_default()
}

pub fn levels_of(source: &str) -> Levels {
    let mut kept = LEVELS.lock().unwrap_or_else(PoisonError::into_inner);
    if let Some((_, levels)) = kept.as_ref().filter(|(known, _)| known == source) {
        return levels.clone();
    }
    let read = script::exported(FILE, source, EXPORT).and_then(|value| Levels::of(&value));
    let levels = read
        .inspect_err(
            |error| tracing::warn!(%error, file = FILE, "the sensitivity levels did not load, so they stay in shadow"),
        )
        .unwrap_or_default();
    *kept = Some((source.to_owned(), levels.clone()));
    levels
}

pub fn mode(levels: &Levels) -> Mode {
    if levels.enforced() { Mode::Enforce } else { Mode::Shadow }
}
