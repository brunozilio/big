#[path = "search_ripgrep.rs"]
mod ripgrep;

use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;

use serde_json::{Map, Value, json};

use super::Reach;
use crate::tools::{Workspace, shown};

pub const SPACE: &str = "search";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    Content,
    Files,
    Count,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Engine {
    BuiltIn,
    Ripgrep(PathBuf),
}

#[cfg(test)]
thread_local! {
    static FORCED: std::cell::RefCell<Option<Engine>> = const { std::cell::RefCell::new(None) };
}

fn engine() -> Engine {
    #[cfg(test)]
    if let Some(forced) = FORCED.with(|forced| forced.borrow().clone()) {
        return forced;
    }
    ripgrep::program().map_or(Engine::BuiltIn, Engine::Ripgrep)
}

pub fn call(reach: &Reach<'_>, name: &str, arguments: &Value) -> Result<Value, String> {
    match name {
        "grep" => grep(reach, arguments),
        other => Err(format!("ctx.{SPACE}.{other} is not a function")),
    }
}

struct Matcher {
    regex: regex::Regex,
    multiline: bool,
}

impl Matcher {
    fn any(&self, text: &str) -> bool {
        if self.multiline { self.regex.is_match(text) } else { text.lines().any(|line| self.regex.is_match(line)) }
    }

    fn count(&self, text: &str) -> usize {
        if self.multiline {
            self.regex.find_iter(text).count()
        } else {
            text.lines().map(|line| self.regex.find_iter(line).count()).sum()
        }
    }

    fn hit_lines(&self, text: &str, lines: usize) -> Vec<usize> {
        if !self.multiline {
            return text.lines().enumerate().filter(|(_, line)| self.regex.is_match(line)).map(|(at, _)| at).collect();
        }
        let starts: Vec<usize> = std::iter::once(0)
            .chain(text.match_indices('\n').map(|(at, _)| at + 1))
            .filter(|at| *at < text.len().max(1))
            .collect();
        let line_of = |offset: usize| starts.partition_point(|start| *start <= offset).saturating_sub(1);
        let mut hits: Vec<usize> = Vec::new();
        for found in self.regex.find_iter(text) {
            let last = if found.end() > found.start() { found.end() - 1 } else { found.start() };
            hits.extend(line_of(found.start())..=line_of(last));
        }
        hits.retain(|line| *line < lines);
        hits.sort_unstable();
        hits.dedup();
        hits
    }
}

struct Asked {
    matcher: Matcher,
    mode: Mode,
    around: (usize, usize),
    most: usize,
}

fn flag(arguments: &Value, key: &str) -> bool {
    arguments.get(key).and_then(Value::as_bool).unwrap_or(false)
}

fn number(arguments: &Value, key: &str) -> usize {
    arguments.get(key).and_then(Value::as_u64).and_then(|n| usize::try_from(n).ok()).unwrap_or(0)
}

fn grep(reach: &Reach<'_>, arguments: &Value) -> Result<Value, String> {
    let pattern = super::text(arguments, "pattern")?;
    let (multiline, ignore_case) = (flag(arguments, "multiline"), flag(arguments, "ignoreCase"));
    let regex = regex::RegexBuilder::new(pattern)
        .case_insensitive(ignore_case)
        .multi_line(true)
        .dot_matches_new_line(multiline)
        .build()
        .map_err(|error| format!("Invalid pattern: {error}"))?;
    let glob = arguments.get("glob").and_then(Value::as_str).filter(|glob| !glob.trim().is_empty());
    let filter = match glob.map(globset::Glob::new) {
        Some(Ok(glob)) => Some(glob.compile_matcher()),
        Some(Err(error)) => return Err(format!("Invalid glob: {error}")),
        None => None,
    };
    let mode = match arguments.get("mode").and_then(Value::as_str) {
        Some("content") => Mode::Content,
        Some("count") => Mode::Count,
        _ => Mode::Files,
    };
    let workspace = reach.workspace;
    let path = arguments.get("path").and_then(Value::as_str);
    reach.fence.reads(&workspace.place(path.unwrap_or(".")).real)?;
    let root = path.map_or_else(|| workspace.cwd.clone(), |path| workspace.resolve(path));
    let wanted = ripgrep::Asked { pattern, ignore_case, multiline };
    let candidates = match engine() {
        Engine::Ripgrep(program) => ripgrep::candidates(&program, &root, &wanted, &workspace.stop),
        Engine::BuiltIn => None,
    };
    let mut files = candidates.unwrap_or_else(|| walk(&root, workspace));
    files.retain(|path| matches_glob(filter.as_ref(), path));
    files.sort();
    files.dedup();
    let around = (number(arguments, "before"), number(arguments, "after"));
    let asked = Asked { matcher: Matcher { regex, multiline }, mode, around, most: number(arguments, "most") };
    Ok(Value::Array(found(&files, &asked, workspace)))
}

fn found(files: &[PathBuf], asked: &Asked, workspace: &Workspace) -> Vec<Value> {
    let mut found = Vec::new();
    for path in files {
        if workspace.stop.load(Ordering::Relaxed) {
            break;
        }
        let Ok(bytes) = super::media::read_bytes(path, &workspace.stop) else { continue };
        let Ok(text) = String::from_utf8(bytes) else { continue };
        let name = shown(path, workspace);
        let matcher = &asked.matcher;
        match asked.mode {
            Mode::Files if matcher.any(&text) => found.push(json!({ "path": name })),
            Mode::Count => {
                let count = matcher.count(&text);
                if count > 0 {
                    found.push(json!({ "path": name, "count": count }));
                }
            }
            Mode::Content => found.extend(lines(asked, &text).map(|mut hit| {
                hit["path"] = json!(name);
                hit
            })),
            Mode::Files => {}
        }
    }
    found
}

fn lines(asked: &Asked, text: &str) -> Option<Value> {
    let all: Vec<&str> = text.lines().collect();
    let hits = asked.matcher.hit_lines(text, all.len());
    if hits.is_empty() {
        return None;
    }
    let (before, after) = asked.around;
    let mut shown = Map::new();
    for &hit in hits.iter().take(asked.most) {
        let end = hit.saturating_add(after).saturating_add(1).min(all.len());
        let start = hit.saturating_sub(before);
        for (at, line) in all.iter().enumerate().take(end).skip(start) {
            shown.insert(at.to_string(), json!(line));
        }
    }
    Some(json!({ "hits": hits, "total": all.len(), "lines": shown }))
}

fn walk(root: &Path, workspace: &Workspace) -> Vec<PathBuf> {
    let walk = ignore::WalkBuilder::new(root)
        .hidden(false)
        .add_custom_ignore_filename(".rgignore")
        .filter_entry(|entry| entry.file_name() != ".git")
        .build();
    let mut files = Vec::new();
    for entry in walk.filter_map(Result::ok) {
        if workspace.stop.load(Ordering::Relaxed) {
            break;
        }
        if entry.file_type().is_some_and(|kind| kind.is_file()) {
            files.push(entry.into_path());
        }
    }
    files
}

fn matches_glob(filter: Option<&globset::GlobMatcher>, path: &Path) -> bool {
    filter.is_none_or(|glob| path.file_name().is_some_and(|name| glob.is_match(name)) || glob.is_match(path))
}

#[cfg(test)]
#[path = "search_tests.rs"]
mod tests;
