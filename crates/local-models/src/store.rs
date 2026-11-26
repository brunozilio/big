use std::path::{Component, Path, PathBuf};

use serde::Serialize;

use crate::LocalError;

pub const PART: &str = ".part";
const DEPTH: usize = 4;

pub fn folder() -> Result<PathBuf, LocalError> {
    let chosen = brand::section("local_models").iter().find(|(key, _)| *key == "folder").map(|(_, value)| value.trim());
    if let Some(chosen) = chosen.filter(|chosen| !chosen.is_empty()) {
        return Ok(match chosen.strip_prefix("~/") {
            Some(rest) => std::env::home_dir().unwrap_or_default().join(rest),
            None => PathBuf::from(chosen),
        });
    }
    platform::local_data_dir(brand::NAME, brand::COMMAND)
        .map(|data| data.join("models"))
        .ok_or_else(|| LocalError::Bad("there is no data folder to keep models in".into()))
}

pub fn relative(name: &str) -> Result<PathBuf, LocalError> {
    let path = Path::new(name);
    let plain = path.components().all(|part| matches!(part, Component::Normal(_)));
    if name.is_empty() || !plain {
        return Err(LocalError::Bad(format!("{name} is not a plain path inside the models folder")));
    }
    Ok(path.to_owned())
}

pub fn resolve(name: &str) -> Result<PathBuf, LocalError> {
    let path = folder()?.join(relative(name)?);
    if path.is_file() { Ok(path) } else { Err(LocalError::Missing(name.to_owned())) }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Stored {
    pub id: String,
    pub bytes: u64,
    pub complete: bool,
}

fn walk(root: &Path, at: &Path, depth: usize, found: &mut Vec<Stored>) {
    let Ok(entries) = std::fs::read_dir(at) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() && depth < DEPTH {
            walk(root, &path, depth + 1, found);
            continue;
        }
        let Ok(inside) = path.strip_prefix(root) else { continue };
        let name = inside.components().map(|part| part.as_os_str().to_string_lossy()).collect::<Vec<_>>().join("/");
        let bytes = entry.metadata().map(|meta| meta.len()).unwrap_or_default();
        match name.strip_suffix(PART) {
            Some(id) => found.push(Stored { id: id.to_owned(), bytes, complete: false }),
            None => found.push(Stored { id: name, bytes, complete: true }),
        }
    }
}

pub fn stored_in(root: &Path) -> Vec<Stored> {
    let mut found = Vec::new();
    walk(root, root, 0, &mut found);
    found.sort_by(|a, b| a.id.cmp(&b.id));
    found
}

pub fn stored() -> Vec<Stored> {
    folder().map(|root| stored_in(&root)).unwrap_or_default()
}

pub fn remove(name: &str) -> Result<(), LocalError> {
    let path = folder()?.join(relative(name)?);
    crate::engine::unload();
    for path in [path.clone(), PathBuf::from(format!("{}{PART}", path.display()))] {
        if path.exists() {
            std::fs::remove_file(&path).map_err(|error| LocalError::Disk(error.to_string()))?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_plain_paths_name_a_model() {
        assert!(relative("owner/repo/model.Q4_K_M.gguf").is_ok());
        assert!(relative("../outside.gguf").is_err());
        assert!(relative("/etc/passwd").is_err());
        assert!(relative("").is_err());
    }

    #[test]
    fn a_part_file_is_a_model_still_arriving() {
        let root = tempfile::tempdir().unwrap_or_else(|error| panic!("{error}"));
        let folder = root.path().join("o/r");
        std::fs::create_dir_all(&folder).unwrap_or_else(|error| panic!("{error}"));
        std::fs::write(folder.join("a.gguf"), b"12").unwrap_or_else(|error| panic!("{error}"));
        std::fs::write(folder.join("b.gguf.part"), b"1").unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(
            stored_in(root.path()),
            [
                Stored { id: "o/r/a.gguf".into(), bytes: 2, complete: true },
                Stored { id: "o/r/b.gguf".into(), bytes: 1, complete: false },
            ]
        );
    }
}
