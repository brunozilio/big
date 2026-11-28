use std::path::{Path, PathBuf};

const FIELD: &str = "schema_version";

struct Kind {
    name: &'static str,
    folder: &'static str,
    wanted: fn(&str) -> bool,
}

fn connector(relative: &str) -> bool {
    relative.split('/').count() == 2 && relative.ends_with("/index.ts")
}

fn top_level(relative: &str) -> bool {
    !relative.contains('/') && (relative.ends_with(".ts") || relative.ends_with(".js"))
}

fn script(relative: &str) -> bool {
    relative.ends_with(".ts") || relative.ends_with(".js")
}

fn json(relative: &str) -> bool {
    relative.ends_with(".json")
}

const KINDS: [Kind; 4] = [
    Kind { name: "connector", folder: "content/connectors", wanted: connector },
    Kind { name: "workflow", folder: "content/workflows", wanted: top_level },
    Kind { name: "package", folder: "content/packages", wanted: script },
    Kind { name: "policy", folder: "content/safety", wanted: json },
];

pub fn findings(root: &Path) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let mut found = Vec::new();
    for kind in KINDS {
        let base = root.join(kind.folder);
        let mut files = Vec::new();
        walk(&base, &mut files);
        for file in files {
            let relative = shown(&base, &file);
            if !(kind.wanted)(&relative) {
                continue;
            }
            if !std::fs::read_to_string(&file)?.contains(FIELD) {
                found.push(format!("contract {}/{relative}: a {} declares no {FIELD}", kind.folder, kind.name));
            }
        }
    }
    Ok(found)
}

fn shown(base: &Path, file: &Path) -> String {
    let relative: PathBuf = file.strip_prefix(base).unwrap_or(file).components().collect();
    relative.to_string_lossy().replace('\\', "/")
}

fn walk(dir: &Path, files: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if entry.file_name().to_string_lossy().starts_with('.') {
            continue;
        }
        if path.is_dir() {
            walk(&path, files);
        } else {
            files.push(path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(root: &Path, path: &str, text: &str) {
        let file = root.join(path);
        std::fs::create_dir_all(file.parent().expect("parent")).expect("dir");
        std::fs::write(file, text).expect("write");
    }

    #[test]
    fn a_contract_without_a_schema_version_is_a_finding() {
        let root = tempfile::tempdir().expect("root");
        write(root.path(), "content/connectors/a/index.ts", "export function provider() { return { name: \"A\" }; }");
        write(root.path(), "content/connectors/a/api.ts", "export const x = 1;");
        write(root.path(), "content/workflows/w.ts", "export default workflow({ schema_version: 1, name: \"w\" });");
        write(root.path(), "content/packages/dev/release.ts", "export default { labels: {} };");
        write(root.path(), "content/safety/p.json", "{ \"schema_version\": 1 }");
        let found = findings(root.path()).expect("findings");
        assert_eq!(
            found,
            [
                "contract content/connectors/a/index.ts: a connector declares no schema_version",
                "contract content/packages/dev/release.ts: a package declares no schema_version",
            ]
        );
    }

    #[test]
    fn every_shipped_contract_declares_its_schema_version() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
        assert_eq!(findings(&root).expect("findings"), Vec::<String>::new());
    }
}
