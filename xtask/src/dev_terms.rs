use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use crate::provider_names::{rust_files, shown};

const CRATES: [&str; 3] = ["crates/domain/src", "crates/orchestrator/src", "crates/server/src"];

const HOST_PRIMITIVES: [&str; 2] = ["crates/domain/src/tools/output/filters/git.rs", "crates/orchestrator/src/git.rs"];

const TERMS: [&str; 10] = [
    "git",
    "pull request",
    "pull requests",
    "pr",
    "prs",
    "branched",
    "base chosen",
    "merged",
    "merge request",
    "merge requests",
];

pub fn findings(root: &Path) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let mut files = Vec::new();
    for folder in CRATES {
        rust_files(&root.join(folder), &mut files);
    }
    let mut found = BTreeSet::new();
    for file in files {
        let shown = shown(root, &file);
        if HOST_PRIMITIVES.contains(&shown.as_str()) {
            continue;
        }
        let source = std::fs::read_to_string(&file)?;
        let code = crate::test_blocks::without_test_modules(&source).map_err(|why| format!("{shown}: {why}"))?;
        let mut seen: BTreeMap<String, usize> = BTreeMap::new();
        for line in code.lines() {
            let words = words(line);
            for term in TERMS.iter().filter(|term| mentions(&words, term)) {
                let finding = format!("dev-term {shown}: {term}: {}", line.trim());
                let count = seen.entry(finding.clone()).or_default();
                *count += 1;
                found.insert(if *count == 1 { finding } else { format!("{finding} (#{count})") });
            }
        }
    }
    Ok(found.into_iter().collect())
}

fn words(line: &str) -> String {
    let mut spaced = String::with_capacity(line.len() + 8);
    let mut lower_before = false;
    for letter in line.chars() {
        if letter.is_ascii_uppercase() && lower_before {
            spaced.push(' ');
        }
        lower_before = letter.is_ascii_lowercase() || letter.is_ascii_digit();
        spaced.push(if letter == '_' { ' ' } else { letter.to_ascii_lowercase() });
    }
    spaced
}

fn mentions(code: &str, term: &str) -> bool {
    let word = |byte: Option<&u8>| byte.is_some_and(u8::is_ascii_alphanumeric);
    code.match_indices(term)
        .any(|(at, _)| !word(code.as_bytes().get(at.wrapping_sub(1))) && !word(code.as_bytes().get(at + term.len())))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_dev_term_is_found_across_case_and_underscores_and_a_longer_word_is_not() {
        let root = tempfile::tempdir().expect("root");
        for folder in ["crates/orchestrator/src", "crates/server/src", "crates/other/src"] {
            std::fs::create_dir_all(root.path().join(folder)).expect("folder");
        }
        let code = "fn a() { let _ = item[\"git\"]; let _ = \"digit\"; let _ = merge_patch; }\n\
                    fn b() { let _ = pr_url; let _ = \"prose\"; }\n\
                    fn c() -> PullRequest { pull_requests(merge_request) }\n\
                    #[cfg(test)]\nmod tests { const X: &str = \"pull_request\"; }\n";
        std::fs::write(root.path().join("crates/server/src/lib.rs"), code).expect("server");
        std::fs::write(root.path().join("crates/orchestrator/src/git.rs"), "fn c() { let _ = \"git\"; }\n")
            .expect("git");
        std::fs::write(root.path().join("crates/orchestrator/src/lib.rs"), "fn d() { let _ = \"git\"; }\n")
            .expect("lib");
        std::fs::write(root.path().join("crates/other/src/lib.rs"), "fn e() { let _ = \"git\"; }\n").expect("other");
        let c = "fn c() -> PullRequest { pull_requests(merge_request) }";
        assert_eq!(
            findings(root.path()).expect("findings"),
            [
                "dev-term crates/orchestrator/src/lib.rs: git: fn d() { let _ = \"git\"; }".to_owned(),
                "dev-term crates/server/src/lib.rs: git: fn a() { let _ = item[\"git\"]; let _ = \"digit\"; let _ = merge_patch; }".to_owned(),
                format!("dev-term crates/server/src/lib.rs: merge request: {c}"),
                "dev-term crates/server/src/lib.rs: pr: fn b() { let _ = pr_url; let _ = \"prose\"; }".to_owned(),
                format!("dev-term crates/server/src/lib.rs: pull request: {c}"),
                format!("dev-term crates/server/src/lib.rs: pull requests: {c}"),
            ]
        );
    }
}
