use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use crate::git;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Checkout {
    pub id: String,
    pub name: String,
    pub directory: Option<PathBuf>,
    pub run: String,
    pub verify: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Worktree {
    pub checkout: Checkout,
    pub path: PathBuf,
}

fn text(value: &Value, name: &str) -> String {
    value[name].as_str().map(str::trim).unwrap_or_default().to_owned()
}

fn checkout(repository: &Value) -> Checkout {
    let directory = Some(text(repository, "directory")).filter(|dir| !dir.is_empty()).map(PathBuf::from);
    let named = text(&repository["code"], "repo");
    let folder = directory.as_deref().and_then(Path::file_name).map(|name| name.to_string_lossy().into_owned());
    let name = if named.is_empty() { folder.unwrap_or_default() } else { named };
    let verify = repository["verify"].as_array().map(Vec::as_slice).unwrap_or_default();
    Checkout {
        id: text(repository, "id"),
        name,
        directory,
        run: text(repository, "run"),
        verify: verify
            .iter()
            .filter_map(Value::as_str)
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(str::to_owned)
            .collect(),
    }
}

pub fn of_input(input: &Value, fallback: Option<&Path>) -> Vec<Checkout> {
    let listed = input["application"]["repositories"].as_array().map(Vec::as_slice).unwrap_or_default();
    let found: Vec<Checkout> = listed.iter().map(checkout).filter(|found| !found.id.is_empty()).collect();
    if !found.is_empty() {
        return found;
    }
    vec![Checkout {
        id: String::new(),
        name: String::new(),
        directory: fallback.map(Path::to_path_buf),
        run: String::new(),
        verify: Vec::new(),
    }]
}

pub fn home(input: &Value, fallback: Option<&Path>) -> Option<PathBuf> {
    of_input(input, fallback).into_iter().next().and_then(|first| first.directory)
}

pub fn folder(branch: &str, id: &str) -> String {
    if id.is_empty() {
        git::folder_for(branch)
    } else {
        format!("{}--{}", git::folder_for(branch), git::folder_for(id))
    }
}

pub fn described(worktrees: &[Worktree]) -> Option<String> {
    if worktrees.iter().all(|worktree| worktree.checkout.id.is_empty()) {
        return None;
    }
    let mut text = String::from("## Repositórios desta tarefa\n");
    for worktree in worktrees {
        let checkout = &worktree.checkout;
        let _ = write!(text, "\n- {}: {}", checkout.name, worktree.path.display());
        if !checkout.run.is_empty() {
            let _ = write!(text, "\n  - Para rodar: `{}`", checkout.run);
        }
        if !checkout.verify.is_empty() {
            let commands: Vec<String> = checkout.verify.iter().map(|command| format!("`{command}`")).collect();
            let _ = write!(text, "\n  - Para verificar: {}", commands.join(", "));
        }
    }
    Some(text)
}

pub fn summary(worktree: &Worktree, base: Option<&str>, commit: Option<&str>) -> Value {
    json!({
        "id": worktree.checkout.id, "name": worktree.checkout.name, "worktree": worktree.path.to_string_lossy(),
        "base": base, "commit": commit, "changed": base != commit,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_item_without_an_application_has_the_default_directory_only() {
        let found = of_input(&json!({ "title": "x" }), Some(Path::new("/padrao")));
        assert_eq!(found.len(), 1);
        assert_eq!((found[0].id.as_str(), found[0].directory.as_deref()), ("", Some(Path::new("/padrao"))));
        assert_eq!(home(&json!(null), None), None);
    }

    #[test]
    fn the_application_s_repositories_come_in_order_with_their_commands() {
        let input = json!({ "application": { "repositories": [
            { "id": "web", "directory": "/src/web", "code": { "connection": "c", "repo": "acme/web" },
              "run": "bun dev", "verify": ["bun test", " "] },
            { "id": "api", "directory": "/src/api" },
        ] } });
        let found = of_input(&input, Some(Path::new("/padrao")));
        assert_eq!(found.iter().map(|found| found.name.as_str()).collect::<Vec<_>>(), ["acme/web", "api"]);
        assert_eq!(found[0].verify, ["bun test"]);
        assert_eq!(home(&input, None), Some(PathBuf::from("/src/web")));
        assert_eq!(folder("cmd/i1", "web"), "cmd_i1--web");
        assert_eq!(folder("cmd/i1", ""), "cmd_i1");
        let worktrees: Vec<Worktree> = found
            .into_iter()
            .map(|checkout| Worktree { path: PathBuf::from("/w").join(&checkout.id), checkout })
            .collect();
        let text = described(&worktrees).expect("described");
        assert!(text.contains("acme/web: /w/web") && text.contains("`bun dev`") && text.contains("`bun test`"));
    }
}
