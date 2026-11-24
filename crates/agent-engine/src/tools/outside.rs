use std::path::{Component, Path, PathBuf};

use super::Workspace;

pub(crate) fn settled(path: &Path) -> PathBuf {
    if let Ok(real) = path.canonicalize() {
        return real;
    }
    let mut out = PathBuf::new();
    for part in path.components() {
        match part {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other),
        }
    }
    let mut existing = out.as_path();
    let mut rest = Vec::new();
    while let Some(parent) = existing.parent() {
        if let Ok(real) = existing.canonicalize() {
            return rest.into_iter().rev().fold(real, |acc: PathBuf, name| acc.join(name));
        }
        rest.extend(existing.file_name());
        existing = parent;
    }
    out
}

fn folded(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::ParentDir => {
                out.pop();
            }
            Component::CurDir => {}
            other => out.push(other),
        }
    }
    out
}

const SHARED_FOLDERS: [&str; 9] = ["agents", "hooks", "prompts", "repl", "schemas", "sdk", "skills", "styles", "tools"];

impl Workspace {
    fn project_root(&self) -> PathBuf {
        let cwd = settled(&self.cwd);
        cwd.ancestors().find(|dir| dir.join(".git").exists()).map_or_else(|| cwd.clone(), Path::to_path_buf)
    }

    pub fn may_read(&self, path: &Path) -> bool {
        let path = settled(&if path.is_absolute() { path.to_path_buf() } else { self.cwd.join(path) });
        self.bounds().may_read(&path)
    }

    pub fn stays_inside(&self, effects: &[domain::permission::Effect]) -> bool {
        let bounds = self.bounds();
        effects.iter().all(|effect| match effect {
            domain::permission::Effect::ReadPath(place) | domain::permission::Effect::ReadTree(place) => {
                bounds.may_read(&place.real)
            }
            _ => true,
        })
    }

    pub fn bounds(&self) -> domain::permission::Bounds {
        let roots = std::iter::once(self.project_root()).chain(self.added_dirs.iter().map(|dir| settled(dir)));
        let saved = settled(&super::persisted::results_dir(&self.session_id));
        let own = content::folder().map(|folder| settled(&folder.join("skills")));
        let shared = self.home.as_ref().map(|home| home.join(brand::DIR)).into_iter().flat_map(|folder| {
            std::iter::once(brand::INSTRUCTIONS).chain(SHARED_FOLDERS).map(move |part| settled(&folder.join(part)))
        });
        domain::permission::Bounds::of(roots.collect(), std::iter::once(saved).chain(own).chain(shared).collect())
    }

    pub fn place(&self, path: &str) -> domain::permission::Place {
        let resolved = self.resolve(path);
        domain::permission::Place { written: folded(&resolved), real: settled(&resolved) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_project_is_inside_and_the_rest_is_not() {
        let home = tempfile::tempdir().expect("dir");
        let project = home.path().join("rocket");
        std::fs::create_dir_all(project.join(".git")).expect("repo");
        std::fs::create_dir_all(project.join("src")).expect("src");
        std::fs::create_dir_all(home.path().join(".ssh")).expect("ssh");
        std::fs::write(home.path().join("notes.md"), "n").expect("notes");
        let workspace = Workspace::new(project.join("src"));
        assert!(workspace.may_read(Path::new("main.rs")), "relative, not there yet");
        assert!(workspace.may_read(&project.join("Cargo.toml")), "the repository's root");
        assert!(!workspace.may_read(&home.path().join(".ssh/id_ed25519")));
        assert!(!workspace.may_read(Path::new("../../.ssh/id_ed25519")), "no climbing out");
        assert!(!workspace.may_read(&home.path().join("notes.md")));
        let read = |path: &str| [domain::permission::Effect::ReadPath(workspace.place(path))];
        assert!(workspace.stays_inside(&read("main.rs")));
        assert!(!workspace.stays_inside(&read("/etc/hosts")));
    }

    #[test]
    fn a_write_outside_or_into_a_protected_folder_is_guarded() {
        use domain::permission::Guard;
        let home = tempfile::tempdir().expect("dir");
        let project = home.path().join("rocket");
        std::fs::create_dir_all(project.join(".git")).expect("repo");
        let workspace = Workspace::new(&project);
        let guard = |path: &str| workspace.bounds().guard(&workspace.place(path).real);
        assert_eq!(guard("src/main.rs"), None);
        assert_eq!(guard("../notes.md"), Some(Guard::OutsideTheProject));
        assert_eq!(guard(".git/hooks/pre-commit"), Some(Guard::GitFolder));
        assert_eq!(guard(&format!("{}/settings.json", brand::DIR)), Some(Guard::OwnConfiguration));
    }

    #[test]
    fn this_session_s_saved_outputs_are_inside_but_another_session_s_are_not() {
        let project = tempfile::tempdir().expect("dir");
        let workspace = Workspace::new(project.path());
        let saved = super::super::persisted::results_dir(&workspace.session_id).join("call_1.txt");
        assert!(workspace.may_read(&saved));
        assert!(!workspace.may_read(&super::super::persisted::results_dir("another").join("call_1.txt")));
    }

    #[test]
    fn the_product_s_own_skills_read_freely_but_are_not_written_without_asking() {
        let installed = tempfile::tempdir().expect("installed");
        let _ = content::use_folder(installed.path().to_path_buf());
        let folder = content::folder().expect("the content folder");
        let project = tempfile::tempdir().expect("dir");
        let workspace = Workspace::new(project.path());
        let skill = folder.join("skills/code-review/medium.md");
        assert!(workspace.may_read(&skill), "an agent reads its own skills");
        assert!(
            workspace.bounds().guard(&workspace.place(&skill.display().to_string()).real).is_some(),
            "writing there still asks"
        );
        assert!(!workspace.may_read(&folder.join("mcp.json")), "the owner's settings stay outside");
    }

    #[test]
    fn the_product_s_home_folder_reads_freely_but_is_not_written_without_asking() {
        let home = tempfile::tempdir().expect("home");
        let own = home.path().join(brand::DIR);
        std::fs::create_dir_all(&own).expect("own folder");
        let project = home.path().join("rocket");
        std::fs::create_dir_all(&project).expect("project");
        let mut workspace = Workspace::new(&project);
        workspace.home = Some(home.path().to_path_buf());
        let instructions = own.join(brand::INSTRUCTIONS);
        assert!(workspace.may_read(&instructions));
        assert!(workspace.may_read(&own.join("agents/reviewer.ts")));
        assert!(!workspace.may_read(&own.join("mcp.json")), "settings may hold secrets");
        assert!(!workspace.may_read(&own.join("teams/team-1/config.json")), "other sessions' teams stay outside");
        let agents = own.join("agents");
        assert!(workspace.stays_inside(&[domain::permission::Effect::ReadTree(workspace.place(&agents.display().to_string()))]));
        assert!(workspace.bounds().guard(&workspace.place(&instructions.display().to_string()).real).is_some());
        assert!(!workspace.may_read(&home.path().join(".ssh/id_ed25519")), "the rest of home stays outside");
    }

    #[test]
    fn an_added_folder_is_inside_like_the_project() {
        let home = tempfile::tempdir().expect("dir");
        let (project, shared) = (home.path().join("rocket"), home.path().join("shared"));
        std::fs::create_dir_all(&project).expect("project");
        std::fs::create_dir_all(shared.join("lib")).expect("shared");
        let mut workspace = Workspace::new(&project);
        workspace.added_dirs = vec![shared.clone()];
        assert!(workspace.may_read(&shared.join("lib/a.rs")));
        assert!(!workspace.may_read(&home.path().join("other/a.rs")));
        assert!(!workspace.may_read(&shared.join("../other/a.rs")), "no climbing out of it either");
    }

    #[cfg(unix)]
    #[test]
    fn a_link_out_of_the_project_is_outside() {
        let home = tempfile::tempdir().expect("dir");
        let project = home.path().join("rocket");
        std::fs::create_dir_all(&project).expect("project");
        std::fs::create_dir_all(home.path().join(".ssh")).expect("ssh");
        std::os::unix::fs::symlink(home.path().join(".ssh"), project.join("keys")).expect("link");
        let workspace = Workspace::new(&project);
        assert!(!workspace.may_read(Path::new("keys/id_ed25519")));
    }
}
