use std::path::{Path, PathBuf};

use super::{RootPick, ServerSpec};

pub fn server_for<'a>(file: &Path, specs: &'a [ServerSpec]) -> Option<(&'a ServerSpec, &'a str)> {
    let extension = format!(".{}", file.extension()?.to_str()?);
    specs.iter().find_map(|spec| spec.extension_to_language.get(&extension).map(|id| (spec, id.as_str())))
}

pub fn root_of(file: &Path, spec: &ServerSpec, cwd: &Path, exists: impl Fn(&Path) -> bool) -> PathBuf {
    let marked = |dir: &&Path| spec.root_markers.iter().any(|marker| exists(&dir.join(marker)));
    let outermost = match spec.root {
        RootPick::Outermost if file.starts_with(cwd) => {
            file.parent().unwrap_or(file).ancestors().take_while(|dir| dir.starts_with(cwd)).filter(marked).last()
        }
        _ => None,
    };
    outermost.map_or_else(|| root_for(file, &spec.root_markers, cwd, &exists), Path::to_path_buf)
}

pub fn root_for(file: &Path, markers: &[String], cwd: &Path, exists: impl Fn(&Path) -> bool) -> PathBuf {
    let inside = file.starts_with(cwd);
    let folder = file.parent().unwrap_or(file);
    let found = folder
        .ancestors()
        .take_while(|dir| !inside || dir.starts_with(cwd))
        .find(|dir| markers.iter().any(|marker| exists(&dir.join(marker))));
    match found {
        Some(dir) => dir.to_path_buf(),
        None if inside => cwd.to_path_buf(),
        None => folder.to_path_buf(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lsp::ServerSpec;

    #[test]
    fn a_file_goes_to_the_server_of_its_extension() {
        let specs: Vec<ServerSpec> = serde_json::from_value(serde_json::json!([
            {"name": "rust-analyzer", "command": "rust-analyzer", "extensionToLanguage": {".rs": "rust"}},
            {"name": "typescript", "command": "tsls", "extensionToLanguage": {".ts": "typescript", ".tsx": "typescriptreact"}}
        ]))
        .expect("specs");
        let (server, language) = server_for(Path::new("/w/web/App.tsx"), &specs).expect("typescript");
        assert_eq!((server.name.as_str(), language), ("typescript", "typescriptreact"));
        assert!(server_for(Path::new("/w/notes.txt"), &specs).is_none());
        assert!(server_for(Path::new("/w/Makefile"), &specs).is_none());
    }

    #[test]
    fn the_root_is_the_closest_marker_but_never_above_the_working_folder() {
        let present = ["/w/Cargo.toml", "/w/crates/a/Cargo.toml", "/Cargo.toml", "/elsewhere/lib/Cargo.toml"];
        let exists = |path: &Path| present.iter().any(|p| Path::new(p) == path);
        let markers = ["Cargo.toml".to_owned()];
        let cwd = Path::new("/w");
        assert_eq!(root_for(Path::new("/w/crates/a/src/lib.rs"), &markers, cwd, exists), Path::new("/w/crates/a"));
        assert_eq!(root_for(Path::new("/w/src/main.rs"), &markers, cwd, exists), Path::new("/w"));
        assert_eq!(root_for(Path::new("/w/tools/x.rs"), &[], cwd, exists), Path::new("/w"));
        assert_eq!(root_for(Path::new("/elsewhere/lib/src/x.rs"), &markers, cwd, exists), Path::new("/elsewhere/lib"));
        assert_eq!(root_for(Path::new("/tmp/x.rs"), &[], cwd, exists), Path::new("/tmp"));
    }

    #[test]
    fn a_server_that_wants_the_outermost_root_gets_the_workspace_inside_the_working_folder() {
        let present = ["/w/Cargo.toml", "/w/crates/a/Cargo.toml", "/Cargo.toml", "/elsewhere/lib/Cargo.toml"];
        let exists = |path: &Path| present.iter().any(|p| Path::new(p) == path);
        let mut spec: ServerSpec = serde_json::from_value(serde_json::json!({
            "name": "ra", "command": "ra", "extensionToLanguage": {".rs": "rust"}, "rootMarkers": ["Cargo.toml"],
            "root": "outermost"
        }))
        .expect("spec");
        let cwd = Path::new("/w");
        assert_eq!(root_of(Path::new("/w/crates/a/src/lib.rs"), &spec, cwd, exists), Path::new("/w"));
        assert_eq!(root_of(Path::new("/elsewhere/lib/src/x.rs"), &spec, cwd, exists), Path::new("/elsewhere/lib"));
        spec.root = RootPick::Closest;
        assert_eq!(root_of(Path::new("/w/crates/a/src/lib.rs"), &spec, cwd, exists), Path::new("/w/crates/a"));
    }
}
