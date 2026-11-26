use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use domain::lsp::ServerSpec;

const DEPTH: usize = 6;
const WALKED: usize = 5000;

pub fn warm(lsp: &crate::lsp::Lsp, cwd: &Path) {
    if !lsp.servers.warms() {
        return;
    }
    let files = first_files(cwd, &lsp.available());
    if files.is_empty() {
        return;
    }
    let servers = Arc::clone(&lsp.servers);
    let specs = Arc::clone(&lsp.specs);
    let cwd = cwd.to_path_buf();
    let started = std::thread::Builder::new().name("lsp-warm".into()).spawn(move || {
        let stop = AtomicBool::new(false);
        for file in files {
            if let Err(error) = servers.open(&file, &specs, &cwd, &stop, crate::lsp::Wait::Never) {
                tracing::debug!(%error, file = %file.display(), "a language server did not warm up");
            }
        }
    });
    if let Err(error) = started {
        tracing::debug!(%error, "the language servers are not warmed up");
    }
}

pub fn first_files(cwd: &Path, specs: &[ServerSpec]) -> Vec<PathBuf> {
    let rooted: Vec<ServerSpec> =
        specs.iter().filter(|spec| spec.root_markers.iter().any(|marker| cwd.join(marker).is_file())).cloned().collect();
    if rooted.is_empty() {
        return Vec::new();
    }
    let mut first: BTreeMap<String, (usize, PathBuf)> = BTreeMap::new();
    let walk = ignore::WalkBuilder::new(cwd).max_depth(Some(DEPTH)).build();
    for entry in walk.take(WALKED).flatten() {
        if !entry.file_type().is_some_and(|kind| kind.is_file()) {
            continue;
        }
        let Some((spec, _)) = domain::lsp::server_for(entry.path(), &rooted) else { continue };
        if domain::lsp::root_of(entry.path(), spec, cwd, Path::is_file) != cwd {
            continue;
        }
        let depth = entry.depth();
        let known = first.get(&spec.name).is_some_and(|(seen, _)| *seen <= depth);
        if !known {
            first.insert(spec.name.clone(), (depth, entry.into_path()));
        }
    }
    first.into_values().map(|(_, file)| file).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(json: serde_json::Value) -> ServerSpec {
        serde_json::from_value(json).expect("spec")
    }

    fn write(dir: &Path, path: &str) {
        let file = dir.join(path);
        std::fs::create_dir_all(file.parent().expect("parent")).expect("dir");
        std::fs::write(file, "").expect("write");
    }

    #[test]
    fn each_server_rooted_at_the_folder_warms_on_its_shallowest_file_under_that_root() {
        let dir = tempfile::tempdir().expect("dir");
        for path in ["Cargo.toml", "crates/a/Cargo.toml", "crates/a/src/lib.rs", "crates/a/src/deep/x.rs", "web/app.ts"] {
            write(dir.path(), path);
        }
        let outermost = spec(serde_json::json!({"name": "ra", "command": "ra", "extensionToLanguage": {".rs": "rust"},
            "rootMarkers": ["Cargo.toml"], "root": "outermost"}));
        let closest = spec(serde_json::json!({"name": "ra", "command": "ra", "extensionToLanguage": {".rs": "rust"},
            "rootMarkers": ["Cargo.toml"]}));
        let unrooted = spec(serde_json::json!({"name": "ts", "command": "ts", "extensionToLanguage": {".ts": "ts"},
            "rootMarkers": ["package.json"]}));
        assert_eq!(first_files(dir.path(), &[outermost, unrooted.clone()]), [dir.path().join("crates/a/src/lib.rs")]);
        assert!(first_files(dir.path(), &[closest]).is_empty(), "a server rooted in a sub-folder is not the folder's");
        assert!(first_files(dir.path(), &[unrooted]).is_empty());
    }
}
