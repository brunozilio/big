use std::borrow::Cow;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Live,
    Installed,
    Embedded,
}

static FOLDER: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();

pub fn use_folder(folder: PathBuf) -> Result<(), PathBuf> {
    FOLDER.set(folder)
}

pub fn installs() -> bool {
    !brand::content::INSTALL.trim().eq_ignore_ascii_case("false")
}

pub fn live_root() -> &'static Path {
    Path::new(super::embedded::ROOT)
}

pub fn mode() -> Mode {
    if FOLDER.get().is_some() {
        return Mode::Installed;
    }
    if cfg!(debug_assertions) && live_root().is_dir() {
        return Mode::Live;
    }
    if installs() { Mode::Installed } else { Mode::Embedded }
}

pub fn home() -> Option<PathBuf> {
    match FOLDER.get() {
        Some(folder) => Some(folder.clone()),
        None => std::env::home_dir().map(|home| home.join(brand::DIR)),
    }
}

pub fn folder() -> Option<PathBuf> {
    if mode() == Mode::Installed { home() } else { None }
}

pub fn skills() -> Option<PathBuf> {
    match mode() {
        Mode::Live => Some(live_root().join("skills")),
        Mode::Installed => {
            super::install_once();
            folder().map(|folder| folder.join("skills"))
        }
        Mode::Embedded => super::install::extracted_skills(),
    }
}

pub fn shipped(path: &str) -> Option<Cow<'static, [u8]>> {
    if super::disabled(path) {
        return None;
    }
    if mode() == Mode::Live
        && let Some(file) = super::override_path(live_root(), path)
        && let Ok(bytes) = std::fs::read(file)
    {
        return Some(Cow::Owned(bytes));
    }
    super::builtin(path).map(Cow::Borrowed)
}

pub fn live_paths(prefix: &str) -> Vec<String> {
    if mode() != Mode::Live {
        return Vec::new();
    }
    let mut found = Vec::new();
    super::modules::walk(live_root(), &live_root().join(prefix), &mut found);
    found.retain(|path| !super::disabled(path));
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_debug_build_reads_the_repository_s_content_as_it_is_now() {
        assert!(live_root().join("prompts").is_dir(), "{}", live_root().display());
        let path = super::super::paths().into_iter().find(|path| path.ends_with(".md")).expect("a text");
        let on_disk = std::fs::read(live_root().join(path)).expect("on disk");
        if mode() == Mode::Live {
            assert_eq!(shipped(path).as_deref(), Some(on_disk.as_slice()));
            assert_eq!(folder(), None, "nothing in the user's folder overrides it");
            assert_eq!(skills(), Some(live_root().join("skills")));
        }
    }
}
