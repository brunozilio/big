use std::path::{Path, PathBuf};

const HOME: &str = "crates/platform/";

const SYSTEM: [&str; 4] = ["HOME", "PATH", "SHELL", "GIT_CONFIG_COUNT"];

const READS: [&str; 2] = ["env::var(", "env::var_os("];

const PRODUCT: &str = "brand::env(";

pub fn findings(root: &Path) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let mut files = Vec::new();
    rust_files(&root.join("crates"), &mut files);
    let mut found = Vec::new();
    for file in files {
        let shown = shown(root, &file);
        if shown.starts_with(HOME) {
            continue;
        }
        for read in reads(&std::fs::read_to_string(&file)?) {
            found.push(format!(
                "env-var {shown}: {read} (configuration lives in config.toml; system variables go through platform)"
            ));
        }
    }
    Ok(found)
}

fn shown(root: &Path, file: &Path) -> String {
    let relative: PathBuf = file.strip_prefix(root).unwrap_or(file).components().collect();
    relative.to_string_lossy().replace('\\', "/")
}

fn reads(source: &str) -> Vec<String> {
    let code = source.split("#[cfg(test)]").next().unwrap_or_default();
    let code: String = code.chars().filter(|letter| !letter.is_whitespace()).collect();
    let mut found: Vec<String> =
        code.match_indices(PRODUCT).map(|_| PRODUCT.trim_end_matches('(').to_owned()).collect();
    for read in READS {
        for (at, _) in code.match_indices(read) {
            let argument = &code[at + read.len()..];
            if !system(argument) {
                found.push(format!("{read}{})", argument.split(')').next().unwrap_or_default()));
            }
        }
    }
    found
}

fn system(argument: &str) -> bool {
    let Some(rest) = argument.strip_prefix('"') else { return false };
    let Some((name, after)) = rest.split_once('"') else { return false };
    after.starts_with(')') && SYSTEM.contains(&name)
}

fn rust_files(dir: &Path, files: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        if path.is_dir() {
            if !["tests", "target", "node_modules"].contains(&name.as_str()) {
                rust_files(&path, files);
            }
        } else if name.ends_with(".rs")
            && name != "build.rs"
            && name != "tests.rs"
            && !name.ends_with("_tests.rs")
            && !name.starts_with("tests_")
        {
            files.push(path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_product_variable_is_found_and_a_system_one_is_left_alone() {
        assert_eq!(reads("fn a() { std::env::var(brand::env(\"LOG\")).ok(); }").len(), 2);
        assert_eq!(reads("fn a() { std::env::var_os(NAME) }"), ["env::var_os(NAME)"]);
        assert_eq!(reads("fn a() { std::env::var(\"RUST_LOG\") }"), ["env::var(\"RUST_LOG\")"]);
        assert!(reads("fn a() { std::env::var_os( \"PATH\" ); env::var(\"HOME\") }").is_empty());
        assert!(reads("fn a() {}\n#[cfg(test)]\nmod tests { fn t() { std::env::var(\"X\"); } }").is_empty());
    }

    #[test]
    fn the_workspace_reads_no_variable_of_its_own_outside_the_baseline() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().expect("workspace root");
        let baseline = std::fs::read_to_string(root.join("xtask/lint-baseline.txt")).unwrap_or_default();
        let found = findings(root).expect("scanned");
        let new: Vec<&String> = found.iter().filter(|finding| !baseline.lines().any(|line| line == *finding)).collect();
        assert!(new.is_empty(), "{new:?}");
    }
}
