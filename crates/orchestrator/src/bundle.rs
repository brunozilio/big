use std::collections::BTreeSet;
use std::path::{Component, Path, PathBuf};

use script::typescript::compile;

use crate::resources::{Resources, script_candidates};

pub const ENTRY: &str = "@entry";

const SDK: &str = "sdk";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bundle {
    pub modules: Vec<(String, String)>,
}

pub fn bundle(resources: &Resources, root: &str) -> Result<Bundle, String> {
    let sdk = resolve(resources, None, SDK)?;
    let mut seen = BTreeSet::new();
    let mut modules = Vec::new();
    visit(resources, &sdk, &mut seen, &mut modules)?;
    visit(resources, root, &mut seen, &mut modules)?;
    let quoted = |text: &str| serde_json::to_string(text).unwrap_or_default();
    modules.push((ENTRY.to_owned(), format!("import {};\nexport {{ host }} from {};\n", quoted(root), quoted(&sdk))));
    Ok(Bundle { modules })
}

fn visit(
    resources: &Resources,
    id: &str,
    seen: &mut BTreeSet<String>,
    modules: &mut Vec<(String, String)>,
) -> Result<(), String> {
    if !seen.insert(id.to_owned()) {
        return Ok(());
    }
    let found = resources.find(id).ok_or_else(|| format!("the module {id} was not found"))?;
    let (code, imports) = compile(id, &found.text, |spec| resolve(resources, Some(id), spec))?;
    for import in imports {
        visit(resources, &import, seen, modules)?;
    }
    modules.push((id.to_owned(), code));
    Ok(())
}

fn resolve(resources: &Resources, from: Option<&str>, spec: &str) -> Result<String, String> {
    let base = if spec.starts_with("./") || spec.starts_with("../") {
        let folder = from.and_then(|from| Path::new(from).parent()).unwrap_or(Path::new(""));
        normalized(&folder.join(spec)).ok_or_else(|| format!("the import {spec} leaves the resource folders"))?
    } else if spec == SDK || spec.starts_with("sdk/") {
        spec.to_owned()
    } else {
        return Err(format!("the import {spec} is not the sdk nor a relative path"));
    };
    let candidates = if base == SDK { script_candidates("sdk/index") } else { script_candidates(&base) };
    let importer = from.unwrap_or(spec);
    content::retired::resolve(
        &content::retired::table(),
        &candidates,
        &|candidate| resources.find(candidate).is_some(),
        importer,
    )
    .ok_or_else(|| format!("the import {spec} was not found"))
}

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
