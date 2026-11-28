use std::path::{Path, PathBuf};

use agent_engine::scope::Scope;
use brand::features as flags;
use domain::features::on;
use domain::memory::{Memory, team};
use domain::redact::Redactor;

pub fn folder(project: &Path) -> PathBuf {
    project.join(brand::DIR).join("memory")
}

pub fn load(gate: &Scope, redactor: &Redactor) -> Vec<Memory> {
    load_in(gate.cwd(), redactor)
}

pub fn load_in(project: &Path, redactor: &Redactor) -> Vec<Memory> {
    if !on(flags::TEAM_MEMORY) {
        return Vec::new();
    }
    let mut files: Vec<PathBuf> = std::fs::read_dir(folder(project))
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "md"))
        .collect();
    files.sort();
    files
        .iter()
        .enumerate()
        .filter_map(|(at, path)| {
            let text = std::fs::read_to_string(path).ok()?;
            let name = path.file_name()?.to_string_lossy().into_owned();
            team::parse(at, &name, &text, redactor)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use domain::memory::{Author, MemoryId, MemoryKind};

    use super::*;

    #[test]
    fn a_shared_memory_is_read_back_by_everyone_in_the_project() {
        let project = tempfile::tempdir().expect("project");
        let mine = Memory {
            id: MemoryId(4),
            kind: MemoryKind::Procedural,
            text: "Run make check before pushing.".into(),
            tags: vec!["ci".into()],
            created: 0,
            origin: None,
            author: Author::Agent,
            history: Vec::new(),
            uses: 0,
            last_used: 0,
            touches: vec!["ci".into()],
            credit: None,
        };

        let gate = Scope::at(project.path(), None);
        let (name, text) = team::file(&mine, &team::Sharer::default());
        std::fs::create_dir_all(folder(project.path())).expect("folder");
        std::fs::write(folder(project.path()).join(name), text).expect("shared");
        let loaded = load(&gate, &Redactor::default());

        assert_eq!(loaded.len(), 1);
        assert!(team::is_team(loaded[0].id));
        assert_eq!(
            (loaded[0].kind, loaded[0].text.as_str()),
            (MemoryKind::Procedural, "Run make check before pushing.")
        );
        assert_eq!(loaded[0].touches, ["ci"]);
    }
}
