use std::collections::BTreeSet;
use std::path::{Component, Path, PathBuf};

use oxc::allocator::Allocator;
use oxc::ast::ast::{Statement, StringLiteral};
use oxc::codegen::Codegen;
use oxc::parser::Parser;
use oxc::semantic::SemanticBuilder;
use oxc::span::SourceType;
use oxc::str::JSStr;
use oxc::transformer::{TransformOptions, Transformer};

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
    candidates
        .into_iter()
        .find(|candidate| resources.find(candidate).is_some())
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

pub fn compile(
    id: &str,
    text: &str,
    mut resolve: impl FnMut(&str) -> Result<String, String>,
) -> Result<(String, Vec<String>), String> {
    let allocator = Allocator::default();
    let typed = id.ends_with(".ts") || id.ends_with(".mts");
    let source_type = if typed { SourceType::ts() } else { SourceType::mjs() };
    let parsed = Parser::new(&allocator, text, source_type).parse();
    if let Some(error) = parsed.diagnostics.iter().next() {
        return Err(format!("{id}: {error}"));
    }
    let mut program = parsed.program;
    let scoping = SemanticBuilder::new().build(&program).semantic.into_scoping();
    let transformed = Transformer::new(&allocator, Path::new(id), &TransformOptions::default())
        .build_with_scoping(scoping, &mut program);
    if let Some(error) = transformed.diagnostics.iter().next() {
        return Err(format!("{id}: {error}"));
    }
    let mut imports = Vec::new();
    for statement in program.body.iter_mut() {
        let Some(source) = source_of(statement) else { continue };
        let spec = source.value.as_str().unwrap_or_default().to_owned();
        let target = resolve(&spec).map_err(|error| format!("{id}: {error}"))?;
        source.value = JSStr::from_str_in(&target, &&allocator);
        source.raw = None;
        imports.push(target);
    }
    Ok((Codegen::new().build(&program).code, imports))
}

fn source_of<'s, 'a>(statement: &'s mut Statement<'a>) -> Option<&'s mut StringLiteral<'a>> {
    match statement {
        Statement::ImportDeclaration(declaration) => Some(&mut declaration.source),
        Statement::ExportAllDeclaration(declaration) => Some(&mut declaration.source),
        Statement::ExportFromDeclaration(declaration) => Some(&mut declaration.source),
        _ => None,
    }
}
