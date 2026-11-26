const ROOT: &str = "sdk/index.js";

fn relative(from: &str, spec: &str) -> String {
    let mut parts: Vec<&str> = from.split('/').collect();
    parts.pop();
    for part in spec.split('/') {
        match part {
            "." => {}
            ".." => {
                parts.pop();
            }
            _ => parts.push(part),
        }
    }
    parts.join("/")
}

fn resolve(from: &str, spec: &str) -> String {
    if spec.starts_with("./") || spec.starts_with("../") {
        return relative(from, spec);
    }
    if spec.ends_with(".js") { spec.to_owned() } else { format!("{spec}/index.js") }
}

fn braced<'a>(line: &'a str, keyword: &str) -> Option<(Vec<(&'a str, &'a str)>, &'a str)> {
    let rest = line.trim().strip_prefix(keyword)?.trim_start().strip_prefix('{')?;
    let (names, rest) = rest.split_once('}')?;
    let spec = rest.trim().strip_prefix("from")?.trim().trim_end_matches(';').trim_matches('"');
    let pairs = names
        .split(',')
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(|name| name.split_once(" as ").unwrap_or((name, name)))
        .collect();
    Some((pairs, spec))
}

fn declared(rest: &str) -> Option<&str> {
    let rest = rest.strip_prefix("async ").unwrap_or(rest);
    let rest = ["const ", "function ", "class ", "let "].iter().find_map(|word| rest.strip_prefix(word))?;
    let end = rest.find(|c: char| !(c.is_alphanumeric() || c == '_' || c == '$')).unwrap_or(rest.len());
    Some(&rest[..end])
}

#[derive(Default)]
pub struct Bundle {
    loaded: Vec<String>,
    out: String,
}

impl Bundle {
    fn load(&mut self, path: &str) -> Result<(), String> {
        if self.loaded.iter().any(|done| done == path) {
            return Ok(());
        }
        self.loaded.push(path.to_owned());
        let text = super::builtin_text(path).ok_or_else(|| format!("{path} is not built in"))?;
        self.module(path, &text)
    }

    fn module(&mut self, path: &str, text: &str) -> Result<(), String> {
        let mut body = String::new();
        let mut names = Vec::new();
        for line in text.lines() {
            if let Some((pairs, spec)) = braced(line, "import") {
                let target = resolve(path, spec);
                self.load(&target)?;
                let bindings: Vec<String> = pairs.iter().map(|(name, alias)| format!("{name}: {alias}")).collect();
                body.push_str(&format!("const {{ {} }} = __m[{target:?}];\n", bindings.join(", ")));
            } else if let Some((pairs, spec)) = braced(line, "export") {
                let target = resolve(path, spec);
                self.load(&target)?;
                for (name, alias) in pairs {
                    body.push_str(&format!("__e.{alias} = __m[{target:?}].{name};\n"));
                }
            } else if let Some(rest) = line.strip_prefix("export default ") {
                body.push_str(&format!("__e.default = {rest}\n"));
            } else if let Some(rest) = line.strip_prefix("export ") {
                names.extend(declared(rest).map(str::to_owned));
                body.push_str(rest);
                body.push('\n');
            } else {
                body.push_str(line);
                body.push('\n');
            }
        }
        let exported: String = names.iter().map(|name| format!("__e.{name} = {name};\n")).collect();
        self.out
            .push_str(&format!("__m[{path:?}] = (() => {{\nconst __e = {{}};\n{body}{exported}return __e;\n}})();\n"));
        Ok(())
    }
}

pub fn bundle(path: &str, text: &str) -> Result<String, String> {
    let runtime = super::builtin_text(ROOT).ok_or("the runtime is built in")?;
    let names: Vec<&str> =
        runtime.lines().filter_map(|line| line.strip_prefix("export ")).filter_map(declared).collect();
    let mut bundle = Bundle { loaded: vec![ROOT.to_owned()], out: String::new() };
    bundle.module(path, text)?;
    Ok(format!("{runtime}\nconst __m = {{}};\n__m[{ROOT:?}] = {{ {} }};\n{}", names.join(", "), bundle.out))
}
