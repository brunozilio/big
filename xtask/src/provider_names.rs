use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

const FOLDERS: [&str; 2] = ["content/connectors", "content/providers"];

const NOT_A_PROVIDER: [&str; 4] = ["mcp", "quick", "prices", "mock_changes"];

pub fn findings(root: &Path) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let names = names(root);
    let mut files = Vec::new();
    rust_files(&root.join("crates"), &mut files);
    let mut found = BTreeSet::new();
    for file in files {
        let shown = shown(root, &file);
        let source = std::fs::read_to_string(&file)?;
        let code = crate::test_blocks::without_test_modules(&source).map_err(|why| format!("{shown}: {why}"))?;
        let mut seen: BTreeMap<String, usize> = BTreeMap::new();
        for line in code.lines() {
            let lowered = line.to_ascii_lowercase();
            for name in names.iter().filter(|name| mentions(&lowered, name)) {
                let finding = format!("provider-name {shown}: {name}: {}", line.trim());
                let count = seen.entry(finding.clone()).or_default();
                *count += 1;
                found.insert(if *count == 1 { finding } else { format!("{finding} (#{count})") });
            }
        }
    }
    Ok(found.into_iter().collect())
}

fn names(root: &Path) -> Vec<String> {
    let mut names = Vec::new();
    for folder in FOLDERS {
        let Ok(entries) = std::fs::read_dir(root.join(folder)) else { continue };
        for entry in entries.flatten().filter(|entry| entry.path().is_dir()) {
            let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
            if !NOT_A_PROVIDER.contains(&name.as_str()) && !name.starts_with('.') {
                names.push(name);
            }
        }
    }
    names.sort();
    names.dedup();
    names
}

fn mentions(code: &str, name: &str) -> bool {
    let word = |byte: Option<&u8>| byte.is_some_and(|byte| byte.is_ascii_alphanumeric());
    code.match_indices(name)
        .any(|(at, _)| !word(code.as_bytes().get(at.wrapping_sub(1))) && !word(code.as_bytes().get(at + name.len())))
}

fn shown(root: &Path, file: &Path) -> String {
    let relative: PathBuf = file.strip_prefix(root).unwrap_or(file).components().collect();
    relative.to_string_lossy().replace('\\', "/")
}

fn rust_files(dir: &Path, files: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        if path.is_dir() {
            if !name.ends_with("tests") && name != "target" && !name.starts_with('.') {
                rust_files(&path, files);
            }
        } else if name.ends_with(".rs") && !name.ends_with("tests.rs") && name != "build.rs" {
            files.push(path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_provider_named_in_the_platform_is_a_finding_and_a_longer_word_is_not() {
        let root = tempfile::tempdir().expect("root");
        std::fs::create_dir_all(root.path().join("content/connectors/acme")).expect("connector");
        std::fs::create_dir_all(root.path().join("content/connectors/mcp")).expect("protocol");
        std::fs::create_dir_all(root.path().join("crates/x/src")).expect("crate");
        let code = "fn a() { let _ = \"acme\"; let _ = \"acmes\"; let _ = \"mcp\"; }\n#[cfg(test)]\nmod tests { const X: &str = \"acme\"; }\n#[cfg(test)]\nfn helper() {}\nfn after() { let _ = \"Acme\"; }\nfn after() { let _ = \"Acme\"; }\n";
        std::fs::write(root.path().join("crates/x/src/lib.rs"), code).expect("lib");
        std::fs::write(root.path().join("crates/x/src/clean.rs"), "fn b() { let _ = \"acmes\"; }\n").expect("clean");
        assert_eq!(
            findings(root.path()).expect("findings"),
            [
                "provider-name crates/x/src/lib.rs: acme: fn a() { let _ = \"acme\"; let _ = \"acmes\"; let _ = \"mcp\"; }",
                "provider-name crates/x/src/lib.rs: acme: fn after() { let _ = \"Acme\"; }",
                "provider-name crates/x/src/lib.rs: acme: fn after() { let _ = \"Acme\"; } (#2)",
            ]
        );
    }
}
