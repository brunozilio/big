use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use crate::provider_names::{rust_files, shown};

const CRATES: [&str; 2] = ["crates/domain/src", "crates/server/src"];

const AGENT_PRIMITIVES: [&str; 6] = [
    "crates/domain/src/knowledge",
    "crates/domain/src/run",
    "crates/domain/src/safety",
    "crates/domain/src/session",
    "crates/domain/src/tools",
    "crates/domain/src/trace",
];

const TERMS: [&str; 9] =
    ["git", "pull_request", "pull request", "pull requests", "pr", "prs", "branched", "base_chosen", "merged"];

pub fn findings(root: &Path) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let mut files = Vec::new();
    for folder in CRATES {
        rust_files(&root.join(folder), &mut files);
    }
    let mut found = BTreeSet::new();
    for file in files {
        let shown = shown(root, &file);
        if AGENT_PRIMITIVES.iter().any(|folder| shown.starts_with(&format!("{folder}/"))) {
            continue;
        }
        let source = std::fs::read_to_string(&file)?;
        let code = crate::test_blocks::without_test_modules(&source).map_err(|why| format!("{shown}: {why}"))?;
        let mut seen: BTreeMap<String, usize> = BTreeMap::new();
        for line in code.lines() {
            let lowered = line.to_ascii_lowercase();
            for term in TERMS.iter().filter(|term| mentions(&lowered, term)) {
                let finding = format!("dev-term {shown}: {term}: {}", line.trim());
                let count = seen.entry(finding.clone()).or_default();
                *count += 1;
                found.insert(if *count == 1 { finding } else { format!("{finding} (#{count})") });
            }
        }
    }
    Ok(found.into_iter().collect())
}

fn mentions(code: &str, term: &str) -> bool {
    let word = |byte: Option<&u8>| byte.is_some_and(|byte| byte.is_ascii_alphanumeric() || *byte == b'_');
    code.match_indices(term)
        .any(|(at, _)| !word(code.as_bytes().get(at.wrapping_sub(1))) && !word(code.as_bytes().get(at + term.len())))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_dev_term_in_the_domain_or_the_server_is_a_finding_and_a_longer_word_is_not() {
        let root = tempfile::tempdir().expect("root");
        for folder in ["crates/domain/src/tools", "crates/server/src", "crates/other/src"] {
            std::fs::create_dir_all(root.path().join(folder)).expect("folder");
        }
        let code = "fn a() { let _ = item[\"git\"]; let _ = \"digit\"; let _ = merge_patch; }\n\
                    fn b() { let _ = run[\"PR\"]; let _ = \"prose\"; }\n\
                    #[cfg(test)]\nmod tests { const X: &str = \"pull_request\"; }\n";
        std::fs::write(root.path().join("crates/server/src/lib.rs"), code).expect("server");
        std::fs::write(root.path().join("crates/domain/src/tools/git.rs"), "fn c() { let _ = \"git\"; }\n")
            .expect("tool");
        std::fs::write(root.path().join("crates/other/src/lib.rs"), "fn d() { let _ = \"git\"; }\n").expect("other");
        assert_eq!(
            findings(root.path()).expect("findings"),
            [
                "dev-term crates/server/src/lib.rs: git: fn a() { let _ = item[\"git\"]; let _ = \"digit\"; let _ = merge_patch; }",
                "dev-term crates/server/src/lib.rs: pr: fn b() { let _ = run[\"PR\"]; let _ = \"prose\"; }",
            ]
        );
    }
}
