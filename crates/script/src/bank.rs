use oxc::allocator::Allocator;
use oxc::ast::ast::{Declaration, Statement};
use oxc::parser::Parser;
use oxc::span::{GetSpan, SourceType};

struct Cut {
    start: usize,
    end: usize,
}

fn declared(declaration: &Declaration<'_>) -> Vec<String> {
    match declaration {
        Declaration::FunctionDeclaration(function) => function.id.iter().map(|id| id.name.to_string()).collect(),
        Declaration::ClassDeclaration(class) => class.id.iter().map(|id| id.name.to_string()).collect(),
        Declaration::VariableDeclaration(variables) => variables
            .declarations
            .iter()
            .filter_map(|variable| variable.id.get_binding_identifier().map(|id| id.name.to_string()))
            .collect(),
        _ => Vec::new(),
    }
}

fn cuts(path: &str, code: &str) -> Result<(Vec<Cut>, Vec<String>), String> {
    let allocator = Allocator::default();
    let parsed = Parser::new(&allocator, code, SourceType::mjs()).parse();
    if let Some(error) = parsed.diagnostics.first() {
        return Err(format!("{path}: {error}"));
    }
    let mut cuts = Vec::new();
    let mut names = Vec::new();
    for statement in &parsed.program.body {
        match statement {
            Statement::ImportDeclaration(_)
            | Statement::ExportAllDeclaration(_)
            | Statement::ExportFromDeclaration(_) => {
                return Err(format!("{path}: a script of the bank cannot import; it runs in the REPL with its tools"));
            }
            Statement::ExportDeclaration(export) => {
                cuts.push(Cut { start: export.span.start as usize, end: export.declaration.span().start as usize });
                names.extend(declared(&export.declaration));
            }
            Statement::ExportNamedDeclaration(_) => {
                return Err(format!("{path}: export each name where it is declared, e.g. export const spec = ..."));
            }
            Statement::ExportDefaultDeclaration(_) => {
                return Err(format!("{path}: a script of the bank exports spec and run, not a default"));
            }
            _ => {}
        }
    }
    Ok((cuts, names))
}

pub fn callable(path: &str, source: &str) -> Result<String, String> {
    let code = crate::typescript::plain(path, source)?;
    let (cuts, names) = cuts(path, &code)?;
    if !names.iter().any(|name| name == "run") {
        return Err(format!("{path}: a script of the bank exports an async function run(input)"));
    }
    let mut body = code;
    for cut in cuts.iter().rev() {
        if body.is_char_boundary(cut.start) && body.is_char_boundary(cut.end) {
            body.replace_range(cut.start..cut.end, "");
        }
    }
    let exported: Vec<String> = names.iter().map(|name| format!("{name}: {name}")).collect();
    Ok(format!("(() => {{\n\"use strict\";\n{body}\nreturn {{ {} }};\n}})()", exported.join(", ")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_script_becomes_an_expression_that_gives_its_exports() {
        let source = "export const spec = { name: \"x\" };\ntype In = { n: number };\nexport async function run(input: In) { return input.n + 1; }\n";
        let code = callable("scripts/x.ts", source).expect("callable");
        assert!(code.starts_with("(() => {") && code.ends_with("return { spec: spec, run: run };\n})()"), "{code}");
        assert!(!code.contains("export") && !code.contains(": In"), "{code}");
    }

    #[test]
    fn a_script_that_imports_or_lacks_run_is_refused() {
        assert!(callable("scripts/a.ts", "import x from \"y\";\nexport async function run() { return x; }").is_err());
        assert!(callable("scripts/b.ts", "export const spec = {};").is_err());
        assert!(callable("scripts/c.ts", "export default async function run() {}").is_err());
        assert!(callable("scripts/d.ts", "export const = ;").is_err());
    }
}
