use std::sync::OnceLock;

pub const ROUTE: &str = "/api/browser/logic.js";

const AGENT: &str = include_str!("../logic/agent.js");

const MODULES: [(&str, &str); 14] = [
    ("results", include_str!("../logic/results.js")),
    ("cdp", include_str!("../logic/cdp.js")),
    ("keys", include_str!("../logic/keys.js")),
    ("sites", include_str!("../logic/sites.js")),
    ("tabs", include_str!("../logic/tabs.js")),
    ("page", include_str!("../logic/page.js")),
    ("computer", include_str!("../logic/computer.js")),
    ("frame", include_str!("../logic/frame.js")),
    ("frames", include_str!("../logic/frames.js")),
    ("logs", include_str!("../logic/logs.js")),
    ("capture", include_str!("../logic/capture.js")),
    ("intercept", include_str!("../logic/intercept.js")),
    ("shortcuts", include_str!("../logic/shortcuts.js")),
    ("main", include_str!("../logic/main.js")),
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Logic {
    pub code: String,
    pub build: String,
}

static BUNDLED: OnceLock<Logic> = OnceLock::new();

pub fn logic() -> &'static Logic {
    BUNDLED.get_or_init(|| {
        let code = bundled();
        let build = crate::extension::fingerprint(&[("logic.js", code.clone())], None);
        Logic { code, build }
    })
}

fn bundled() -> String {
    let config = serde_json::json!({
        "NAME": brand::NAME,
        "VERSION": env!("CARGO_PKG_VERSION"),
        "BUILD": "",
        "PORT": 0,
    });
    let mut code = String::from("(() => {\n\"use strict\";\nconst __m = {};\n");
    code.push_str(&format!("__m[\"config\"] = {config};\n"));
    code.push_str(&format!("__m[\"agent\"] = {{ AGENT: {} }};\n", serde_json::json!(brand::text(AGENT))));
    for (name, source) in MODULES {
        code.push_str(&module(name, &brand::text(source)));
    }
    code.push_str("globalThis.__logic = __m[\"main\"].handlers;\n})();\n");
    code
}

fn module(name: &str, source: &str) -> String {
    let mut body = String::new();
    let mut exported = Vec::new();
    for line in source.lines() {
        body.push_str(&converted(line, &mut exported));
        body.push('\n');
    }
    format!("__m[{name:?}] = (() => {{\n{body}return {{ {} }};\n}})();\n", exported.join(", "))
}

fn converted(line: &str, exported: &mut Vec<String>) -> String {
    if let Some(imported) = imported(line) {
        return imported;
    }
    let Some(rest) = line.strip_prefix("export ") else { return line.to_owned() };
    if let Some(name) = declared(rest) {
        exported.push(name);
    }
    rest.to_owned()
}

fn imported(line: &str) -> Option<String> {
    let rest = line.strip_prefix("import ")?.trim_end().strip_suffix(';')?;
    let (names, from) = rest.split_once(" from ")?;
    let key = from.trim_matches('"').rsplit('/').next()?.strip_suffix(".js")?;
    if let Some(alias) = names.strip_prefix("* as ") {
        return Some(format!("const {} = __m[{key:?}];", alias.trim()));
    }
    let listed = names.trim().strip_prefix('{')?.strip_suffix('}')?;
    let bound: Vec<String> = listed
        .split(',')
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(|name| match name.split_once(" as ") {
            Some((original, alias)) => format!("{}: {}", original.trim(), alias.trim()),
            None => name.to_owned(),
        })
        .collect();
    Some(format!("const {{ {} }} = __m[{key:?}];", bound.join(", ")))
}

fn declared(rest: &str) -> Option<String> {
    let rest = rest.strip_prefix("async ").unwrap_or(rest);
    let rest =
        ["function* ", "function ", "const ", "let ", "class "].iter().find_map(|kind| rest.strip_prefix(kind))?;
    let name: String = rest.chars().take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '$').collect();
    (!name.is_empty()).then_some(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn imports_and_exports_become_the_module_table() {
        let mut exported = Vec::new();
        assert_eq!(
            converted(r#"import { a, b as c } from "./x.js";"#, &mut exported),
            r#"const { a, b: c } = __m["x"];"#
        );
        assert_eq!(
            converted(r#"import * as s from "./shortcuts.js";"#, &mut exported),
            r#"const s = __m["shortcuts"];"#
        );
        assert_eq!(
            converted(r#"import { NAME } from "../config.js";"#, &mut exported),
            r#"const { NAME } = __m["config"];"#
        );
        assert_eq!(converted("export async function go(x) {", &mut exported), "async function go(x) {");
        assert_eq!(converted("export const handlers = {", &mut exported), "const handlers = {");
        assert_eq!(
            converted("export class ToolError extends Error {", &mut exported),
            "class ToolError extends Error {"
        );
        assert_eq!(exported, ["go", "handlers", "ToolError"]);
    }

    #[test]
    fn every_module_imports_only_what_comes_before_it_and_nothing_is_left_as_a_module() {
        let mut known = vec!["config".to_owned(), "agent".to_owned()];
        for (name, source) in MODULES {
            for line in source.lines().filter(|line| line.starts_with("import ")) {
                let made = imported(line).unwrap_or_default();
                let key = made.rsplit("__m[\"").next().unwrap_or_default().trim_end_matches("\"];");
                assert!(known.iter().any(|known| known == key), "{name} imports {key} before it is bundled: {line}");
            }
            known.push(name.to_owned());
        }
        let code = &logic().code;
        assert!(!code.lines().any(|line| line.starts_with("import ") || line.starts_with("export ")));
        assert!(code.contains("globalThis.__logic = __m[\"main\"].handlers;"));
        assert_eq!(logic().build.len(), 16);
    }

    #[test]
    fn every_file_of_the_logic_folder_is_bundled() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("logic");
        for entry in std::fs::read_dir(&root).into_iter().flatten().flatten() {
            let file = entry.file_name().to_string_lossy().into_owned();
            let stem = file.trim_end_matches(".js");
            assert!(stem == "agent" || MODULES.iter().any(|(name, _)| *name == stem), "not bundled: {file}");
        }
    }
}
