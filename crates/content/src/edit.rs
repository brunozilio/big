use std::path::{Path, PathBuf};

use super::Mode;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    Shipped,
    Edited,
    Own,
}

impl Origin {
    pub fn name(self) -> &'static str {
        match self {
            Origin::Shipped => "shipped",
            Origin::Edited => "edited",
            Origin::Own => "own",
        }
    }
}

pub fn editing_root() -> Option<PathBuf> {
    match super::mode() {
        Mode::Live => Some(super::live_root().to_path_buf()),
        Mode::Installed => {
            super::install_once();
            super::folder()
        }
        Mode::Embedded => None,
    }
}

fn base(path: &str, live: bool) -> Option<Vec<u8>> {
    if super::disabled(path) {
        return None;
    }
    let bytes = super::builtin(path)?;
    Some(if live { bytes.to_vec() } else { super::install::branded(bytes) })
}

fn on_disk(root: &Path, path: &str) -> Option<Vec<u8>> {
    std::fs::read(super::override_path(root, path)?).ok()
}

pub fn origin_in(root: &Path, path: &str, live: bool) -> Origin {
    match (base(path, live), on_disk(root, path)) {
        (None, _) => Origin::Own,
        (Some(base), Some(current)) if base != current => Origin::Edited,
        _ => Origin::Shipped,
    }
}

pub fn editable_in(root: &Path, path: &str, live: bool) -> Option<String> {
    on_disk(root, path).or_else(|| base(path, live)).and_then(|bytes| String::from_utf8(bytes).ok())
}

pub fn write_in(root: &Path, path: &str, text: &str) -> std::io::Result<()> {
    let target = super::override_path(root, path).ok_or_else(|| std::io::Error::other("not a plain path"))?;
    super::install::write_atomically(&target, text.as_bytes())
}

pub fn restore_in(root: &Path, path: &str, live: bool) -> std::io::Result<()> {
    let target = super::override_path(root, path).ok_or_else(|| std::io::Error::other("not a plain path"))?;
    if let Some(base) = base(path, live) {
        return super::install::write_atomically(&target, &base);
    }
    match std::fs::remove_file(&target) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    if let Some(parent) = target.parent().filter(|parent| *parent != root) {
        let _ = std::fs::remove_dir(parent);
    }
    Ok(())
}

fn live() -> bool {
    super::mode() == Mode::Live
}

pub fn origin(path: &str) -> Origin {
    editing_root().map_or(Origin::Shipped, |root| origin_in(&root, path, live()))
}

pub fn editable(path: &str) -> Option<String> {
    match editing_root() {
        Some(root) => editable_in(&root, path, live()),
        None => super::shipped(path).and_then(|bytes| String::from_utf8(bytes.into_owned()).ok()),
    }
}

fn root() -> std::io::Result<PathBuf> {
    editing_root().ok_or_else(|| std::io::Error::other("this installation keeps no content folder to edit"))
}

pub fn write(path: &str, text: &str) -> std::io::Result<()> {
    write_in(&root()?, path, text)
}

pub fn restore(path: &str) -> std::io::Result<()> {
    restore_in(&root()?, path, live())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shipped_text() -> &'static str {
        crate::paths().into_iter().find(|path| path.starts_with("agents/")).expect("an agent")
    }

    #[test]
    fn an_edit_marks_the_file_and_restoring_brings_the_shipped_one_back() {
        let root = tempfile::tempdir().expect("root");
        let path = shipped_text();
        assert_eq!(origin_in(root.path(), path, false), Origin::Shipped);
        let shipped = editable_in(root.path(), path, false).expect("shipped text");
        assert!(!shipped.contains("{{NAME}}"), "an installed copy carries the product's name");
        write_in(root.path(), path, "mine").expect("write");
        assert_eq!(origin_in(root.path(), path, false), Origin::Edited);
        assert_eq!(editable_in(root.path(), path, false).as_deref(), Some("mine"));
        restore_in(root.path(), path, false).expect("restore");
        assert_eq!(origin_in(root.path(), path, false), Origin::Shipped);
        assert_eq!(editable_in(root.path(), path, false), Some(shipped));
    }

    #[test]
    fn a_file_of_the_user_s_own_is_removed_with_its_empty_folder() {
        let root = tempfile::tempdir().expect("root");
        write_in(root.path(), "skills/mine/SKILL.md", "---\nname: mine\n---\n").expect("write");
        assert_eq!(origin_in(root.path(), "skills/mine/SKILL.md", false), Origin::Own);
        restore_in(root.path(), "skills/mine/SKILL.md", false).expect("remove");
        assert!(!root.path().join("skills/mine").exists());
        restore_in(root.path(), "skills/mine/SKILL.md", false).expect("already gone");
    }

    #[test]
    fn a_path_that_leaves_the_folder_is_refused() {
        let root = tempfile::tempdir().expect("root");
        assert!(write_in(root.path(), "../outside.ts", "no").is_err());
        assert!(restore_in(root.path(), "/etc/hosts", false).is_err());
    }
}
