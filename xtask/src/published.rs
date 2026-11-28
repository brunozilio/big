use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};

pub const LEDGER: &str = "content/published.json";
const RETIRED: &str = "content/retired.json";
const ROUTES: &str = "crates/server/src";
const ROUTE: &str = ".route(\"";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Retired {
    pub from: String,
    pub to: String,
}

pub struct Ledgers<'a> {
    pub current: &'a BTreeSet<String>,
    pub ledger: &'a BTreeSet<String>,
    pub before: &'a BTreeSet<String>,
    pub retired: &'a [Retired],
}

pub fn check(given: &Ledgers) -> Vec<String> {
    let retired: BTreeSet<&str> = given.retired.iter().map(|retired| retired.from.as_str()).collect();
    let gone = |id: &&String| !given.current.contains(*id) && !retired.contains(id.as_str());
    let mut found: Vec<String> = given
        .current
        .difference(given.ledger)
        .map(|id| format!("published-id {id}: new id, add it with `xtask conformance --publish`"))
        .collect();
    found.extend(given.ledger.iter().filter(gone).map(|id| {
        format!("published-id {id}: removed without retirement; keep it, or retire it in {RETIRED} (ids) after migrating")
    }));
    found.extend(given.before.difference(given.ledger).filter(|id| !retired.contains(id.as_str())).map(|id| {
        format!("published-id {id}: left {LEDGER}; a published id is retired, never deleted from the ledger")
    }));
    for entry in given.retired.iter().filter(|entry| !entry.to.is_empty() && !given.current.contains(&entry.to)) {
        found.push(format!("published-id {}: retired in favor of {}, which is not published", entry.from, entry.to));
    }
    found
}

pub fn ids_of(text: &str) -> Result<BTreeSet<String>, String> {
    let value: Value = serde_json::from_str(text).map_err(|error| format!("{LEDGER} does not read: {error}"))?;
    Ok(value["ids"].as_array().into_iter().flatten().filter_map(Value::as_str).map(str::to_owned).collect())
}

pub fn retired_of(text: &str) -> Result<Vec<Retired>, String> {
    let value: Value = serde_json::from_str(text).map_err(|error| format!("{RETIRED} does not read: {error}"))?;
    let listed = value["ids"].as_array().into_iter().flatten();
    Ok(listed
        .filter_map(|entry| {
            let from = entry["from"].as_str().filter(|from| !from.is_empty())?;
            Some(Retired { from: from.to_owned(), to: entry["to"].as_str().unwrap_or_default().to_owned() })
        })
        .collect())
}

pub fn ledger_text(ids: &BTreeSet<String>) -> String {
    let text = serde_json::to_string_pretty(&json!({ "schema_version": 1, "ids": ids })).unwrap_or_default();
    format!("{text}\n")
}

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
        let code = crate::test_blocks::without_test_modules(&source).map_err(|why| format!("{}: {why}", file.display()))?;
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

fn committed(root: &Path) -> BTreeSet<String> {
    let shown = std::process::Command::new("git")
        .current_dir(root)
        .args(["show", &format!("HEAD:{LEDGER}")])
        .output()
        .ok()
        .filter(|output| output.status.success());
    shown.and_then(|output| ids_of(&String::from_utf8_lossy(&output.stdout)).ok()).unwrap_or_default()
}

pub fn findings(root: &Path, current: &BTreeSet<String>) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let ledger = ids_of(&std::fs::read_to_string(root.join(LEDGER)).unwrap_or_else(|_| "{}".into()))?;
    let retired = retired_of(&std::fs::read_to_string(root.join(RETIRED))?)?;
    let before = committed(root);
    Ok(check(&Ledgers { current, ledger: &ledger, before: &before, retired: &retired }))
}

pub fn publish(root: &Path, current: &BTreeSet<String>) -> Result<usize, Box<dyn std::error::Error>> {
    let path = root.join(LEDGER);
    let mut ledger = ids_of(&std::fs::read_to_string(&path).unwrap_or_else(|_| "{}".into()))?;
    let before = ledger.len();
    ledger.extend(current.iter().cloned());
    std::fs::write(path, ledger_text(&ledger))?;
    Ok(ledger.len() - before)
}

#[cfg(test)]
mod tests;
