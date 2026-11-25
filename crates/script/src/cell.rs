use oxc::allocator::Allocator;
use oxc::ast::ast::{Statement, VariableDeclarationKind};
use oxc::parser::{ParseOptions, Parser};
use oxc::span::SourceType;

fn parses(code: &str, allow_return: bool) -> bool {
    let allocator = Allocator::default();
    let options = ParseOptions { allow_return_outside_function: allow_return, ..ParseOptions::default() };
    Parser::new(&allocator, code, SourceType::mjs()).with_options(options).parse().diagnostics.is_empty()
}

struct Change {
    start: usize,
    end: usize,
    text: String,
}

fn change(statement: &Statement<'_>) -> Vec<Change> {
    match statement {
        Statement::VariableDeclaration(declaration) => {
            let length = match declaration.kind {
                VariableDeclarationKind::Let => 3,
                VariableDeclarationKind::Const => 5,
                _ => return Vec::new(),
            };
            let start = declaration.span.start as usize;
            vec![Change { start, end: start + length, text: "var".to_owned() }]
        }
        Statement::ClassDeclaration(class) if class.decorators.is_empty() => {
            let Some(name) = &class.id else { return Vec::new() };
            let (start, end) = (class.span.start as usize, class.span.end as usize);
            vec![
                Change { start, end: start, text: format!("var {} = ", name.name) },
                Change { start: end, end, text: ";".to_owned() },
            ]
        }
        _ => Vec::new(),
    }
}

fn declared_again(code: &str) -> String {
    let allocator = Allocator::default();
    let parsed = Parser::new(&allocator, code, SourceType::mjs()).parse();
    let mut changes: Vec<Change> = parsed.program.body.iter().flat_map(change).collect();
    changes.sort_by_key(|change| (change.start, change.end));
    let mut rewritten = code.to_owned();
    for change in changes.into_iter().rev() {
        if rewritten.is_char_boundary(change.start) && rewritten.is_char_boundary(change.end) {
            rewritten.replace_range(change.start..change.end, &change.text);
        }
    }
    rewritten
}

pub fn persistent(code: &str) -> String {
    if parses(code, false) {
        return declared_again(code);
    }
    if parses(code, true) {
        return format!("(async () => {{\n{code}\n}})()");
    }
    code.to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_cell_may_declare_again_what_an_earlier_cell_declared() {
        assert_eq!(persistent("const r = 1;\nlet s = r + 1;"), "var r = 1;\nvar s = r + 1;");
        assert_eq!(persistent("for (const x of [1]) { const y = x; }"), "for (const x of [1]) { const y = x; }");
        assert_eq!(persistent("const s = \"const t = 1\";"), "var s = \"const t = 1\";");
    }

    #[test]
    fn a_class_becomes_a_variable_holding_it() {
        assert_eq!(
            persistent("class Point { x = 1 }\nnew Point().x"),
            "var Point = class Point { x = 1 };\nnew Point().x"
        );
        assert_eq!(persistent("const a = 1; class B {}"), "var a = 1; var B = class B {};");
    }

    #[test]
    fn a_cell_that_returns_at_its_top_runs_as_a_function_and_a_broken_one_is_left_alone() {
        assert_eq!(persistent("if (x) return 1;\nreturn 2;"), "(async () => {\nif (x) return 1;\nreturn 2;\n})()");
        assert_eq!(persistent("const = ;"), "const = ;");
        assert_eq!(persistent("await ctx.read('a')"), "await ctx.read('a')");
    }
}
