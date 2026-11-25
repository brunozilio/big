use std::path::{Path, PathBuf};

use serde_json::Value;

use super::{Interceptor, Interceptors, Level, default_timeout, entries};
use crate::hooks::BUILTIN_SCRIPT;

const FOLDER: &str = "intercept";

const TOOL_SUFFIXES: [&str; 2] = [".intercept.ts", ".intercept.js"];

fn scripted(path: &Path) -> bool {
    path.extension().is_some_and(|ext| ext == "ts" || ext == "js") && !path.to_string_lossy().ends_with(".d.ts")
}

fn scripts_in(folder: &Path) -> Vec<PathBuf> {
    let Ok(read) = std::fs::read_dir(folder) else { return Vec::new() };
    let mut files: Vec<PathBuf> = read.flatten().map(|entry| entry.path()).filter(|path| scripted(path)).collect();
    files.sort();
    files
}

pub fn tool_named(file: &Path) -> Option<&str> {
    let name = file.file_name()?.to_str()?;
    TOOL_SUFFIXES.iter().find_map(|suffix| name.strip_suffix(suffix)).filter(|name| !name.is_empty())
}

fn timeout_of(value: Option<&Value>) -> u64 {
    value
        .and_then(Value::as_u64)
        .filter(|seconds| crate::hooks::TIMEOUT_SECONDS.contains(seconds))
        .unwrap_or_else(default_timeout)
}

fn global(shown: &str, source: &str) -> Interceptors {
    let read = |name: &str| script::exported(shown, source, name).ok().filter(|value| !value.is_null());
    if !script::exports(shown, source, "default") {
        tracing::warn!(file = shown, "an interceptor has no default export");
        return Interceptors::default();
    }
    let matcher = read("matcher").and_then(|value| value.as_str().map(str::to_owned)).unwrap_or_default();
    let one = Interceptor::new(shown, matcher, timeout_of(read("timeout").as_ref()), Level::Global);
    let kinds = match read("kind") {
        Some(Value::String(kind)) => vec![kind],
        Some(Value::Array(kinds)) => kinds.iter().filter_map(Value::as_str).map(str::to_owned).collect(),
        _ => vec!["tool".to_owned()],
    };
    let mut found = Interceptors::default();
    for kind in kinds {
        match kind.as_str() {
            "tool" => found.tool.push(one.clone()),
            "model" => found.model.push(one.clone()),
            other => tracing::warn!(file = shown, kind = other, "an interceptor names an unknown kind"),
        }
    }
    found
}

fn tool_file(file: &Path) -> Option<Interceptor> {
    let name = tool_named(file)?;
    let shown = file.display().to_string();
    let source = std::fs::read_to_string(file).ok()?;
    let timeout = timeout_of(script::exported(&shown, &source, "timeout").ok().as_ref());
    Some(Interceptor::new(shown, regex::escape(name), timeout, Level::Tool))
}

pub fn of_tool(name: &str, runs_from: &str, shown: &str, source: &str) -> Result<Vec<Interceptor>, String> {
    if !source.contains("intercept") {
        return Ok(Vec::new());
    }
    let outline = script::outline(shown, source, "intercept")?;
    if outline.is_null() {
        return Ok(Vec::new());
    }
    let matcher = regex::escape(name);
    Ok(entries(&outline, "intercept")?
        .into_iter()
        .map(|entry| {
            let run = format!("{runs_from}#{}", entry.export);
            Interceptor::new(run, matcher.clone(), timeout_of(entry.timeout.as_ref()), Level::Tool)
        })
        .collect())
}

impl Interceptors {
    pub fn gathered(home: Option<&Path>, cwd: &Path, roots: &[PathBuf], runs: &dyn Fn(&Path) -> bool) -> Self {
        let user = home.map(|home| home.join(brand::DIR).join(FOLDER));
        let replaced = |path: &str| {
            let name = Path::new(path).file_name();
            user.as_ref().is_some_and(|folder| name.is_some_and(|name| folder.join(name).is_file()))
        };
        let mut found = Self::default();
        for (path, bytes) in content::builtin_files(&format!("{FOLDER}/")) {
            if scripted(Path::new(path)) && !replaced(path) {
                found.extend(&global(
                    &format!("{BUILTIN_SCRIPT}{path}"),
                    &brand::text(&String::from_utf8_lossy(&bytes)),
                ));
            }
        }
        for file in roots.iter().flat_map(|root| scripts_in(&root.join(FOLDER))).filter(|file| runs(file)) {
            if let Ok(source) = std::fs::read_to_string(&file) {
                found.extend(&global(&file.display().to_string(), &source));
            }
        }
        let tool_folders = home.map(|home| home.join(brand::DIR)).into_iter().chain([cwd.join(brand::DIR)]);
        let tool_files = tool_folders.flat_map(|root| scripts_in(&root.join("tools")));
        found.tool.extend(tool_files.filter(|file| runs(file)).filter_map(|file| tool_file(&file)));
        found
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn global_files_tool_files_and_a_tool_s_own_export_are_each_read_at_their_level() {
        let home = tempfile::tempdir().expect("home");
        let project = tempfile::tempdir().expect("project");
        let user = home.path().join(brand::DIR);
        std::fs::create_dir_all(user.join(FOLDER)).expect("intercept");
        std::fs::create_dir_all(project.path().join(brand::DIR).join("tools")).expect("tools");
        let both = "export const kind = [\"tool\", \"model\"];\nexport const matcher = \"Read\";\nexport default (v, ctx, next) => next(v);";
        std::fs::write(user.join(FOLDER).join("audit.js"), both).expect("global");
        let wrap = "export const timeout = 7;\nexport default (v, ctx, next) => next(v);";
        std::fs::write(project.path().join(brand::DIR).join("tools").join("Bash.intercept.ts"), wrap).expect("tool");
        let roots = [user.clone()];
        let found = Interceptors::gathered(Some(home.path()), project.path(), &roots, &|_| true);
        assert_eq!(found.model.len(), 1);
        assert_eq!((found.tool[0].matcher.as_str(), found.tool[0].level), ("Read", Level::Global));
        let bash = found.tool.iter().find(|one| one.level == Level::Tool).expect("tool level");
        assert_eq!((bash.matcher.as_str(), bash.timeout), ("Bash", 7));
        let refused =
            Interceptors::gathered(Some(home.path()), project.path(), &roots, &|file| file.starts_with(&user));
        assert!(refused.tool.iter().all(|one| one.level == Level::Global), "an untrusted project adds nothing");

        let source = "export const spec = { name: \"Deploy\" };\nexport const intercept = [{ timeout: 3, run: (v, ctx, next) => next(v) }];";
        let own = of_tool("Deploy", "/t/deploy.ts", "/t/deploy.ts", source).expect("own");
        assert_eq!(own, [Interceptor::new("/t/deploy.ts#intercept.0.run", "Deploy", 3, Level::Tool)]);
        assert!(of_tool("Read", "r.ts", "r.ts", "export const spec = {};").expect("none").is_empty());
    }
}
