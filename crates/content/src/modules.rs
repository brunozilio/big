use std::collections::BTreeSet;
use std::path::{Component, Path, PathBuf};

use script::typescript::compile_named;

const SDK: &str = "sdk/";
const ENDINGS: [&str; 5] = ["", ".ts", ".js", "/index.ts", "/index.js"];

fn normalized(path: &Path) -> Option<String> {
    let mut parts = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Normal(part) => parts.push(part),
            Component::ParentDir => {
                if !parts.pop() {
                    return None;
                }
            }
            Component::CurDir => {}
            Component::RootDir | Component::Prefix(_) => return None,
        }
    }
    Some(parts.to_string_lossy().replace('\\', "/"))
}

fn exists(folder: Option<&Path>, path: &str) -> bool {
    super::shipped(path).is_some()
        || folder.and_then(|folder| super::override_path(folder, path)).is_some_and(|file| file.is_file())
}

fn resolve(folder: Option<&Path>, from: &str, spec: &str, names: &[String]) -> Result<String, String> {
    let base = if spec.starts_with("./") || spec.starts_with("../") {
        let parent = Path::new(from).parent().unwrap_or(Path::new(""));
        normalized(&parent.join(spec)).ok_or_else(|| format!("the import {spec} leaves the content folder"))?
    } else if spec.starts_with(SDK) {
        spec.to_owned()
    } else {
        return Err(format!("the import {spec} is neither the sdk nor a relative path"));
    };
    let candidates: Vec<String> = ENDINGS
        .iter()
        .map(|ending| format!("{base}{ending}"))
        .filter(|candidate| candidate.ends_with(".ts") || candidate.ends_with(".js"))
        .collect();
    let found =
        super::retired::resolve(&super::retired::table(), &candidates, &|candidate| exists(folder, candidate), from)
            .ok_or_else(|| format!("the import {spec} of {from} was not found"))?;
    super::moved::warned(&super::moved::table(), &found, names, from);
    Ok(found)
}

fn source_in(folder: Option<&Path>, path: &str) -> Option<String> {
    super::user_text(folder, path).or_else(|| super::builtin_text(path))
}

fn visit(
    folder: Option<&Path>,
    path: &str,
    seen: &mut BTreeSet<String>,
    modules: &mut Vec<(String, String)>,
) -> Result<(), String> {
    if !seen.insert(path.to_owned()) {
        return Ok(());
    }
    let text = source_in(folder, path).ok_or_else(|| format!("the module {path} was not found"))?;
    let (code, imports) = compile_named(path, &text, |spec, names| resolve(folder, path, spec, names))?;
    for import in imports {
        visit(folder, &import, seen, modules)?;
    }
    modules.push((path.to_owned(), code));
    Ok(())
}

pub fn modules_in(folder: Option<&Path>, entry: &str) -> Result<Vec<(String, String)>, String> {
    let mut seen = BTreeSet::new();
    let mut modules = Vec::new();
    visit(folder, entry, &mut seen, &mut modules)?;
    Ok(modules)
}

pub fn modules(entry: &str) -> Result<Vec<(String, String)>, String> {
    modules_in(super::folder().as_deref(), entry)
}

pub(crate) fn walk(root: &Path, dir: &Path, found: &mut Vec<String>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.file_name().is_some_and(|name| name.to_string_lossy().starts_with('.')) {
            continue;
        }
        if path.is_dir() {
            walk(root, &path, found);
        } else if let Ok(relative) = path.strip_prefix(root) {
            found.push(relative.to_string_lossy().replace('\\', "/"));
        }
    }
}

pub fn listed_in(folder: Option<&Path>, prefix: &str) -> Vec<String> {
    let mut all: BTreeSet<String> =
        super::paths().into_iter().filter(|path| path.starts_with(prefix)).map(str::to_owned).collect();
    all.extend(super::place::live_paths(prefix));
    if let Some(folder) = folder {
        let mut found = Vec::new();
        walk(folder, &folder.join(prefix), &mut found);
        all.extend(found);
    }
    all.into_iter().collect()
}

pub fn listed(prefix: &str) -> Vec<String> {
    listed_in(super::folder().as_deref(), prefix)
}

pub fn bytes_in(folder: Option<&Path>, path: &str) -> Option<Vec<u8>> {
    let mine = folder.and_then(|folder| super::override_path(folder, path)).and_then(|file| std::fs::read(file).ok());
    mine.or_else(|| super::shipped(path).map(std::borrow::Cow::into_owned))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(folder: &Path, path: &str, text: &str) {
        let file = folder.join(path);
        std::fs::create_dir_all(file.parent().expect("parent")).expect("dir");
        std::fs::write(file, text).expect("write");
    }

    #[test]
    fn a_module_brings_its_relative_and_sdk_imports_before_itself() {
        let folder = tempfile::tempdir().expect("folder");
        write(folder.path(), "sdk/zz/help.ts", "export const two: number = 2;\n");
        write(
            folder.path(),
            "things/a/lib.ts",
            "import { two } from \"sdk/zz/help.ts\";\nexport const three = two + 1;\n",
        );
        write(folder.path(), "things/a/index.ts", "import { three } from \"./lib\";\nexport const four = three + 1;\n");
        let found = modules_in(Some(folder.path()), "things/a/index.ts").expect("modules");
        let names: Vec<&str> = found.iter().map(|(name, _)| name.as_str()).collect();
        assert_eq!(names, ["sdk/zz/help.ts", "things/a/lib.ts", "things/a/index.ts"]);
        assert!(found[2].1.contains("\"things/a/lib.ts\""), "{}", found[2].1);
        assert!(listed_in(Some(folder.path()), "things/").contains(&"things/a/index.ts".to_owned()));
    }

    #[test]
    fn an_import_outside_the_folder_or_of_a_package_is_refused() {
        let folder = tempfile::tempdir().expect("folder");
        write(folder.path(), "things/b.ts", "import x from \"../../etc\";\nexport default x;\n");
        write(folder.path(), "things/c.ts", "import x from \"left-pad\";\nexport default x;\n");
        assert!(modules_in(Some(folder.path()), "things/b.ts").is_err());
        assert!(modules_in(Some(folder.path()), "things/c.ts").is_err());
    }
}
