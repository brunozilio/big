use std::path::{Path, PathBuf};

use domain::Folder;

use crate::tools::settled;

fn is_repository(dir: &Path) -> bool {
    dir.join(".git").exists()
}

pub fn canonical(folder: &Path) -> PathBuf {
    settled(folder)
}

pub fn root_of(cwd: &Path, home: Option<&Path>) -> PathBuf {
    let home = home.map(canonical);
    let cwd = canonical(cwd);
    let counts = |dir: &Path| is_repository(dir) && dir.parent().is_some() && home.as_deref() != Some(dir);
    cwd.ancestors().find(|dir| counts(dir)).map_or_else(|| cwd.clone(), Path::to_path_buf)
}

#[derive(Debug, Clone)]
pub struct Scope {
    cwd: PathBuf,
    home: Option<PathBuf>,
    user: Option<PathBuf>,
    root: PathBuf,
}

impl Scope {
    pub fn at(cwd: &Path, home: Option<&Path>) -> Self {
        Self {
            cwd: cwd.to_path_buf(),
            home: home.map(Path::to_path_buf),
            user: home.map(|home| settled(&home.join(brand::DIR))),
            root: root_of(cwd, home),
        }
    }

    pub fn of(folder: Folder<'_>) -> Self {
        Self::at(folder.cwd, folder.home())
    }

    pub fn within(folder: Folder<'_>, home: Option<&Path>) -> Self {
        Self::at(folder.cwd, home)
    }

    pub fn cwd(&self) -> &Path {
        &self.cwd
    }

    pub fn home(&self) -> Option<&Path> {
        self.home.as_deref()
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn is_the_user_s(&self, file: &Path) -> bool {
        let file = settled(file);
        self.user.as_deref().is_some_and(|user| file.starts_with(user))
    }

    pub fn is_the_project_s(&self, file: &Path) -> bool {
        !self.is_the_user_s(file) && settled(file).starts_with(&self.root)
    }

    pub fn honours(&self, file: &Path) -> bool {
        self.is_the_user_s(file) || self.is_the_project_s(file)
    }

    pub fn project_folders(&self) -> Vec<PathBuf> {
        settled(&self.cwd)
            .ancestors()
            .take_while(|dir| dir.starts_with(&self.root))
            .map(|dir| dir.join(brand::DIR))
            .filter(|folder| !self.is_the_user_s(folder))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_project_s_root_is_its_repository_and_both_the_user_s_and_its_files_are_honoured() {
        let dir = tempfile::tempdir().expect("dir");
        let home = dir.path().join("home");
        let project = home.join("work/rocket");
        std::fs::create_dir_all(project.join(".git")).expect("repo");
        std::fs::create_dir_all(project.join("src")).expect("src");
        std::fs::create_dir_all(home.join(brand::DIR)).expect("user folder");
        let scope = Scope::at(&project.join("src"), Some(&home));
        assert_eq!(scope.root(), canonical(&project));
        assert!(scope.honours(&project.join(brand::DIR).join("settings.json")));
        assert!(scope.honours(&home.join(brand::DIR).join("settings.json")));
        assert!(!scope.honours(&home.join("elsewhere.json")));
        assert_eq!(scope.project_folders().len(), 2);
    }

    #[test]
    fn the_home_folder_is_never_a_project_s_root() {
        let dir = tempfile::tempdir().expect("dir");
        let home = dir.path().join("home");
        std::fs::create_dir_all(home.join(".git")).expect("dotfiles repo");
        std::fs::create_dir_all(home.join("notes")).expect("notes");
        assert_eq!(root_of(&home.join("notes"), Some(&home)), canonical(&home.join("notes")));
    }
}
