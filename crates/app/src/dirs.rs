use std::path::{Path, PathBuf};

pub fn app_data_dir() -> Option<PathBuf> {
    platform::data_dir(brand::NAME, brand::COMMAND)
}

pub fn local_data_dir() -> Option<PathBuf> {
    platform::local_data_dir(brand::NAME, brand::COMMAND)
}

pub fn project_key(project: &std::path::Path) -> String {
    project.to_string_lossy().chars().map(|c| if c.is_alphanumeric() { c } else { '-' }).collect()
}

pub fn memory_key(project: &Path) -> String {
    project_key(&main_checkout(project))
}

fn main_checkout(project: &Path) -> PathBuf {
    let Ok(marker) = std::fs::read_to_string(project.join(".git")) else { return project.to_path_buf() };
    let Some(pointed) = marker.trim().strip_prefix("gitdir:").map(str::trim) else { return project.to_path_buf() };
    let pointed = project.join(pointed);
    let worktrees = pointed.parent().filter(|dir| dir.file_name().is_some_and(|name| name == "worktrees"));
    let git = worktrees.and_then(Path::parent).filter(|dir| dir.file_name().is_some_and(|name| name == ".git"));
    git.and_then(Path::parent).map_or_else(|| project.to_path_buf(), Path::to_path_buf)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_worktree_keeps_its_memory_with_the_repository_it_came_from() {
        let root = tempfile::tempdir().expect("dir");
        let repo = root.path().join("repo");
        std::fs::create_dir_all(repo.join(".git/worktrees/item-1")).expect("git");
        let tree = root.path().join("trees/item-1");
        std::fs::create_dir_all(&tree).expect("tree");
        let pointer = format!("gitdir: {}\n", repo.join(".git/worktrees/item-1").display());
        std::fs::write(tree.join(".git"), pointer).expect("pointer");
        assert_eq!(memory_key(&tree), project_key(&repo));
        assert_eq!(memory_key(&repo), project_key(&repo));
        assert_eq!(memory_key(Path::new("/nowhere")), project_key(Path::new("/nowhere")));
    }
}
