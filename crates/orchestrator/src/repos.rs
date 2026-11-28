use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use crate::git;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Checkout {
    pub id: String,
    pub name: String,
    pub directory: Option<PathBuf>,
    pub kind: String,
    pub run: Vec<String>,
    pub verify: Vec<String>,
    pub e2e: Vec<String>,
    pub e2e_directory: String,
    pub read_only: bool,
}

impl Checkout {
    pub fn scratch(directory: &Path) -> Checkout {
        Checkout {
            id: String::new(),
            name: String::new(),
            directory: Some(directory.to_path_buf()),
            kind: String::new(),
            run: Vec::new(),
            verify: Vec::new(),
            e2e: Vec::new(),
            e2e_directory: String::new(),
            read_only: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Worktree {
    pub checkout: Checkout,
    pub path: PathBuf,
    pub branch: String,
}

fn text(value: &Value, name: &str) -> String {
    value[name].as_str().map(str::trim).unwrap_or_default().to_owned()
}

fn commands(value: &Value, name: &str) -> Vec<String> {
    if let Some(line) = value[name].as_str() {
        return Some(line.trim()).filter(|line| !line.is_empty()).map(str::to_owned).into_iter().collect();
    }
    let listed = value[name].as_array().map(Vec::as_slice).unwrap_or_default();
    listed.iter().filter_map(Value::as_str).map(str::trim).filter(|line| !line.is_empty()).map(str::to_owned).collect()
}

fn quoted(commands: &[String]) -> String {
    commands.iter().map(|command| format!("`{command}`")).collect::<Vec<_>>().join(", ")
}

fn checkout(repository: &Value) -> Checkout {
    let directory = Some(text(repository, "directory")).filter(|dir| !dir.is_empty()).map(PathBuf::from);
    let named = text(&repository["bindings"][connectors::contracts::CODE], "value");
    let folder = directory.as_deref().and_then(Path::file_name).map(|name| name.to_string_lossy().into_owned());
    let name = if named.is_empty() { folder.unwrap_or_default() } else { named };
    Checkout {
        id: text(repository, "id"),
        name,
        directory,
        kind: text(repository, "kind"),
        run: commands(repository, "run"),
        verify: commands(repository, "verify"),
        e2e: commands(repository, "e2e"),
        e2e_directory: text(repository, "e2eDirectory"),
        read_only: repository["readOnly"].as_bool() == Some(true),
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
        kind: String::new(),
        run: Vec::new(),
        verify: Vec::new(),
        e2e: Vec::new(),
        e2e_directory: String::new(),
        read_only: false,
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
    let mut text = String::from("## Repositories of this task\n");
    for worktree in worktrees {
        let checkout = &worktree.checkout;
        let _ = write!(text, "\n- {}{}: {}", checkout.name, kind_note(&checkout.kind), worktree.path.display());
        if checkout.read_only {
            text.push_str(READ_ONLY_NOTE);
        }
        if !checkout.run.is_empty() {
            let _ = write!(text, "\n  - To run: {}", quoted(&checkout.run));
        }
        if !checkout.verify.is_empty() {
            let _ = write!(text, "\n  - To verify: {}", quoted(&checkout.verify));
        }
        if !checkout.e2e.is_empty() {
            let place = if checkout.e2e_directory.is_empty() {
                String::new()
            } else {
                format!(" in `{}`", checkout.e2e_directory)
            };
            let _ = write!(text, "\n  - End-to-end suite{place}: {}", quoted(&checkout.e2e));
        }
    }
    let _ = write!(
        text,
        "\n\nWhen a repository has no command listed, discover from its files how it runs, is verified and runs its \
         end-to-end suite. If you create a repository or service that is not in this list, or find out how one of \
         them runs or is verified, record it with `{}` before you finish.",
        crate::application::ADD_REPOSITORY
    );
    Some(text)
}

const READ_ONLY_NOTE: &str = "\n  - Read only: consult it, but never edit, commit, branch or push in it.";

fn kind_note(kind: &str) -> String {
    if kind.is_empty() { String::new() } else { format!(" ({kind} repository)") }
}

pub fn summary(worktree: &Worktree, base: Option<&str>, commit: Option<&str>) -> Value {
    if worktree.checkout.read_only {
        return json!({
            "id": worktree.checkout.id, "name": worktree.checkout.name, "worktree": worktree.path.to_string_lossy(),
            "readOnly": true, "changed": false,
        });
    }
    json!({
        "id": worktree.checkout.id, "name": worktree.checkout.name, "worktree": worktree.path.to_string_lossy(),
        "branch": worktree.branch, "base": base, "commit": commit, "changed": base != commit,
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
            { "id": "web", "directory": "/src/web", "kind": "application", "bindings": { "code": { "connection": "c", "value": "acme/web" } },
              "run": "bun dev", "verify": ["bun test", " "] },
            { "id": "app", "directory": "/src/app", "run": ["bun api", " bun web ", ""] },
            { "id": "api", "directory": "/src/api" },
        ] } });
        let found = of_input(&input, Some(Path::new("/padrao")));
        assert_eq!(found.iter().map(|found| found.name.as_str()).collect::<Vec<_>>(), ["acme/web", "app", "api"]);
        assert_eq!(found[0].verify, ["bun test"]);
        assert_eq!(found[0].run, ["bun dev"]);
        assert_eq!(found[1].run, ["bun api", "bun web"]);
        assert!(found[2].run.is_empty());
        assert_eq!(home(&input, None), Some(PathBuf::from("/src/web")));
        assert_eq!(folder("cmd/i1", "web"), "cmd_i1--web");
        assert_eq!(folder("cmd/i1", ""), "cmd_i1");
        let worktrees: Vec<Worktree> = found
            .into_iter()
            .map(|checkout| Worktree { path: PathBuf::from("/w").join(&checkout.id), branch: String::new(), checkout })
            .collect();
        let text = described(&worktrees).expect("described");
        assert!(
            text.contains("acme/web (application repository): /w/web")
                && text.contains("`bun dev`")
                && text.contains("`bun test`")
        );
        assert!(text.contains("  - To run: `bun api`, `bun web`"));
        assert!(text.contains("`AddRepository`") && text.contains("app: /w/app"));
    }

    #[test]
    fn a_read_only_repository_is_consulted_without_branch_or_commit() {
        let input = json!({ "application": { "repositories": [
            { "id": "cfg", "directory": "/src/cfg", "readOnly": true },
            { "id": "api", "directory": "/src/api" },
        ] } });
        let found = of_input(&input, None);
        assert_eq!(found.iter().map(|found| found.read_only).collect::<Vec<_>>(), [true, false]);
        let worktrees: Vec<Worktree> = found
            .into_iter()
            .map(|checkout| Worktree { path: PathBuf::from("/w").join(&checkout.id), branch: "b".into(), checkout })
            .collect();
        let text = described(&worktrees).expect("described");
        assert!(text.contains("cfg: /w/cfg\n  - Read only:"));
        assert!(!text.contains("api: /w/api\n  - Read only:"));
        let consulted = summary(&worktrees[0], Some("a"), Some("a"));
        assert_eq!(consulted["readOnly"], json!(true));
        assert_eq!(consulted["changed"], json!(false));
        assert!(consulted.get("commit").is_none() && consulted.get("branch").is_none());
        assert_eq!(summary(&worktrees[1], Some("a"), Some("b"))["changed"], json!(true));
    }
}
