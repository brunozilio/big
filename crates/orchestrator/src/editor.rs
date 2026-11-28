use std::path::{Path, PathBuf};

use domain::workflow::{ItemId, WorkItem};
use platform::editor::Folder;

use crate::api::Orchestrator;
use crate::error::Result;
use crate::git;
use crate::repos::{self, Checkout};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItemFolders {
    pub workspace: PathBuf,
    pub folders: Vec<Folder>,
}

fn place(item: &WorkItem, checkout: &Checkout, data: &Path) -> Option<PathBuf> {
    let tree = data.join("worktrees").join(repos::folder(&git::head_of(item, &checkout.id), &checkout.id));
    Some(tree).filter(|tree| tree.is_dir()).or_else(|| checkout.directory.clone().filter(|dir| dir.is_dir()))
}

fn named(checkout: &Checkout, path: &Path) -> String {
    let folder = || path.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default();
    Some(checkout.name.clone()).filter(|name| !name.is_empty()).unwrap_or_else(folder)
}

pub(crate) fn found(item: &WorkItem, checkouts: &[Checkout], data: &Path) -> Vec<Folder> {
    let mut folders: Vec<Folder> = Vec::new();
    for checkout in checkouts {
        let Some(path) = place(item, checkout, data) else { continue };
        if folders.iter().all(|folder| folder.path != path) {
            folders.push(Folder { name: named(checkout, &path), path });
        }
    }
    folders
}

impl Orchestrator {
    pub fn folders(&self, id: &ItemId) -> Result<Option<ItemFolders>> {
        let Some(item) = self.shared.store.item(id)? else { return Ok(None) };
        let data = &self.shared.config.data;
        let folders = found(&item, &self.shared.checkouts(&item), data);
        let workspace = data.join("workspaces").join(git::folder_for(item.id.as_str()));
        Ok(Some(ItemFolders { workspace, folders }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn checkout(id: &str, name: &str, directory: Option<&Path>) -> Checkout {
        Checkout {
            id: id.into(),
            name: name.into(),
            directory: directory.map(Path::to_path_buf),
            kind: String::new(),
            run: Vec::new(),
            verify: Vec::new(),
            e2e: Vec::new(),
            read_only: false,
            e2e_directory: String::new(),
        }
    }

    fn item() -> WorkItem {
        WorkItem {
            id: ItemId::from("item-1"),
            bindings: vec![domain::workflow::Binding::new("", crate::bindings::LANE, serde_json::json!("big/x"))],
            ..WorkItem::default()
        }
    }

    #[test]
    fn the_worktree_wins_over_the_local_directory() {
        let data = tempfile::tempdir().expect("data");
        let local = tempfile::tempdir().expect("local");
        let tree = data.path().join("worktrees").join(repos::folder("big/x", "api"));
        std::fs::create_dir_all(&tree).expect("tree");
        let listed = [checkout("api", "api", Some(local.path())), checkout("web", "", Some(local.path()))];
        let folders = found(&item(), &listed, data.path());
        let named = local.path().file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default();
        assert_eq!(
            folders,
            vec![Folder { name: "api".into(), path: tree }, Folder { name: named, path: local.path().to_path_buf() }]
        );
    }

    #[test]
    fn a_repository_without_a_folder_is_left_out() {
        let data = tempfile::tempdir().expect("data");
        let listed = [checkout("api", "api", Some(Path::new("/nowhere/at/all"))), checkout("web", "web", None)];
        assert!(found(&item(), &listed, data.path()).is_empty());
    }
}
