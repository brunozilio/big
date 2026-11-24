use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::Path;
use std::sync::{Mutex, OnceLock};

use oxc::allocator::Allocator;
use oxc::ast::ast::{Statement, StringLiteral};
use oxc::codegen::Codegen;
use oxc::parser::Parser;
use oxc::semantic::SemanticBuilder;
use oxc::span::SourceType;
use oxc::str::JSStr;
use oxc::transformer::{TransformOptions, Transformer};

const MOST_KEPT: usize = 512;

pub fn typed(id: &str) -> bool {
    id.ends_with(".ts") || id.ends_with(".mts")
}

pub fn compile(
    id: &str,
    text: &str,
    mut resolve: impl FnMut(&str) -> Result<String, String>,
) -> Result<(String, Vec<String>), String> {
    let allocator = Allocator::default();
    let source_type = if typed(id) { SourceType::ts() } else { SourceType::mjs() };
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

fn compiled() -> &'static Mutex<HashMap<u64, String>> {
    static CACHE: OnceLock<Mutex<HashMap<u64, String>>> = OnceLock::new();
    CACHE.get_or_init(Default::default)
}

pub fn plain(id: &str, text: &str) -> Result<String, String> {
    if !typed(id) {
        return Ok(text.to_owned());
    }
    let mut hasher = DefaultHasher::new();
    (id, text).hash(&mut hasher);
    let key = hasher.finish();
    if let Some(code) = compiled().lock().ok().and_then(|cache| cache.get(&key).cloned()) {
        return Ok(code);
    }
    let (code, _) = compile(id, text, |spec| Ok(spec.to_owned()))?;
    if let Ok(mut cache) = compiled().lock() {
        if cache.len() >= MOST_KEPT {
            cache.clear();
        }
        cache.insert(key, code.clone());
    }
    Ok(code)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_typed_module_loses_its_types_and_keeps_its_imports() {
        let source = "import { a } from \"./a\";\nexport const spec: { name: string } = { name: \"x\" };\nexport default (n: number): number => n + a;";
        let (code, imports) = compile("tools/x.ts", source, |spec| Ok(format!("{spec}.ts"))).expect("compiled");
        assert_eq!(imports, ["./a.ts"]);
        assert!(code.contains("\"./a.ts\"") && !code.contains(": number"), "{code}");
        assert_eq!(plain("tools/x.js", "let a: number = 1;").expect("as is"), "let a: number = 1;");
        let plain_code = plain("tools/y.ts", "export const n: number = 1;").expect("plain");
        assert!(!plain_code.contains(": number"));
        assert_eq!(plain("tools/y.ts", "export const n: number = 1;").expect("cached"), plain_code);
        assert!(plain("tools/z.ts", "export const = ;").is_err());
    }
}
