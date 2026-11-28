use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

const ROUTES: &str = "crates/server/src";
const ROUTE: &str = ".route(\"";

pub fn routes_in(source: &str) -> Vec<String> {
    let code: String = source.split_whitespace().collect::<Vec<_>>().join("");
    let mut found = Vec::new();
    let mut rest = code.as_str();
    while let Some(at) = rest.find(ROUTE) {
        rest = &rest[at + ROUTE.len()..];
        if let Some(end) = rest.find('"') {
            found.push(format!("route:{}", &rest[..end]));
        }
    }
    found
}

pub fn routes(root: &Path) -> Result<BTreeSet<String>, Box<dyn std::error::Error>> {
    let mut files = Vec::new();
    rust_files(&root.join(ROUTES), &mut files);
    let mut found = BTreeSet::new();
    for file in files {
        let source = std::fs::read_to_string(&file)?;
        let code =
            crate::test_blocks::without_test_modules(&source).map_err(|why| format!("{}: {why}", file.display()))?;
        found.extend(routes_in(&code));
    }
    Ok(found)
}

fn rust_files(dir: &Path, files: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        if path.is_dir() {
            rust_files(&path, files);
        } else if name.ends_with(".rs") && !name.starts_with("tests") && !name.ends_with("_tests.rs") {
            files.push(path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn literal_routes_are_read_across_lines_and_constants_are_skipped() {
        let source = "Router::new()\n  .route(\"/api/items\", get(list))\n  .route(\n    \"/api/items/{id}\",\n    get(one))\n  .route(ROUTE, get(x))";
        assert_eq!(routes_in(source), ["route:/api/items", "route:/api/items/{id}"]);
    }

    #[test]
    fn the_shipped_server_routes_are_found() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
        assert!(routes(&root).expect("routes").contains("route:/api/items"));
    }
}
