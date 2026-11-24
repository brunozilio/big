use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use axum::extract::State;
use axum::response::Response;
use domain::permission::{Allowed, Call, Effect, Grant, Suggested, allowed};
use domain::conversation::Entry;
use serde_json::{Value, json};

use crate::error::{ApiError, ApiResult, json as reply};
use crate::session_view::Shown;
use crate::state::{Ctx, Shared};

const SESSIONS: usize = 25;
const FOLDERS: usize = 4;
const FILE_LIMIT: usize = 3000;
const SEEN_LIMIT: usize = 80;

type Ran = Vec<(String, Vec<Entry>)>;

#[derive(Default)]
struct Seen {
    times: usize,
    sessions: BTreeSet<String>,
}

fn used(entry: &Entry) -> Vec<Grant> {
    let Entry::Tool { name, input: Some(input), .. } = entry else { return Vec::new() };
    if let Some(command) = input["command"].as_str().filter(|_| name == "Bash") {
        let effects = [Effect::exec(command)];
        return Allowed::learned(&Call { tool: name, subject: "", effects: &effects });
    }
    if let Some(site) = input["url"].as_str().and_then(domain::site::site) {
        return vec![Grant::Site(site)];
    }
    let browser = format!("mcp__{}__", domain::mcp::browser_server());
    if name.starts_with("mcp__") && !name.starts_with(&browser) {
        return vec![Grant::Tool(name.clone())];
    }
    Vec::new()
}

fn tally(sessions: &[(String, Vec<Entry>)]) -> Vec<(Grant, Seen)> {
    let mut seen: BTreeMap<String, (Grant, Seen)> = BTreeMap::new();
    for (session, entries) in sessions {
        for grant in entries.iter().flat_map(used) {
            let (_, count) = seen.entry(grant.rule()).or_insert_with(|| (grant.clone(), Seen::default()));
            count.times += 1;
            count.sessions.insert(session.clone());
        }
    }
    let mut seen: Vec<(Grant, Seen)> = seen.into_values().collect();
    seen.sort_by_key(|(_, count)| std::cmp::Reverse((count.sessions.len(), count.times)));
    seen.truncate(SEEN_LIMIT);
    seen
}

fn clipped(text: &str) -> String {
    text.chars().take(FILE_LIMIT).collect()
}

fn read(folder: &Path, name: &str) -> Option<String> {
    std::fs::read_to_string(folder.join(name)).ok().filter(|text| !text.trim().is_empty())
}

fn workflow_runs(folder: &Path) -> Option<String> {
    let listed = std::fs::read_dir(folder.join(".github/workflows")).ok()?;
    let runs: Vec<String> = listed
        .flatten()
        .filter_map(|file| std::fs::read_to_string(file.path()).ok())
        .flat_map(|text| {
            text.lines()
                .filter(|line| line.trim_start().starts_with("run:"))
                .map(str::trim)
                .map(str::to_owned)
                .collect::<Vec<_>>()
        })
        .collect();
    (!runs.is_empty()).then(|| runs.join("\n"))
}

fn project(folder: &Path) -> String {
    let scripts = read(folder, "package.json")
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
        .map(|package| package["scripts"].to_string())
        .filter(|scripts| scripts != "null");
    let parts = [
        ("package.json scripts", scripts),
        ("Cargo.toml", read(folder, "Cargo.toml")),
        (".cargo/config.toml", read(folder, ".cargo/config.toml")),
        ("Makefile", read(folder, "Makefile")),
        ("justfile", read(folder, "justfile")),
        ("CI run steps", workflow_runs(folder)),
        ("CLAUDE.md", read(folder, "CLAUDE.md")),
    ];
    let found: Vec<String> = parts
        .into_iter()
        .filter_map(|(label, text)| text.map(|text| format!("### {label}\n{}", clipped(&text))))
        .collect();
    if found.is_empty() { String::new() } else { format!("## Project {}\n{}", folder.display(), found.join("\n\n")) }
}

fn evidence(seen: &[(Grant, Seen)], folders: &[PathBuf], current: &Allowed) -> String {
    let ran: Vec<String> = seen
        .iter()
        .map(|(grant, count)| format!("- {} ({}× in {} sessions)", grant.rule(), count.times, count.sessions.len()))
        .collect();
    let projects: Vec<String> = folders.iter().map(|folder| project(folder)).filter(|text| !text.is_empty()).collect();
    let listed: Vec<String> =
        current.rules().iter().map(|rule| format!("{} ({:?})", rule.grant.rule(), rule.behavior)).collect();
    format!(
        "# Used in recent sessions\n{}\n\n# Projects\n{}\n\n# Already on the list (Allow, Ask or Deny: never propose these again)\n{}",
        ran.join("\n"),
        projects.join("\n\n"),
        listed.join("\n"),
    )
}

fn fresh(found: Vec<Suggested>, current: &Allowed) -> Vec<Suggested> {
    found
        .into_iter()
        .filter(|suggested| !current.has(&suggested.grant))
        .filter(|suggested| match &suggested.grant {
            Grant::Command(command) => {
                let effects = [Effect::exec(command)];
                !current.covers(&Call { tool: "Bash", subject: "", effects: &effects })
            }
            Grant::Site(site) => site.starts_with("*.") || !current.site_allowed(site),
            Grant::Tool(_) => true,
        })
        .collect()
}

fn recent(shared: &Shared) -> ApiResult<(Ran, Vec<PathBuf>)> {
    let mut rows = shared.backend.client.sessions().map_err(ApiError::failed)?;
    rows.sort_by_key(|row| std::cmp::Reverse(row.updated));
    rows.truncate(SESSIONS);
    let mut folders: Vec<PathBuf> = Vec::new();
    let mut sessions = Vec::new();
    for row in rows {
        if !folders.contains(&row.cwd) && folders.len() < FOLDERS {
            folders.push(row.cwd.clone());
        }
        let Ok(records) = shared.backend.client.history(&row.session, &row.cwd) else { continue };
        sessions.push((row.session.0.clone(), Shown::of(&records).transcript.entries));
    }
    Ok((sessions, folders))
}

fn discovered(shared: &Shared) -> ApiResult<Vec<Suggested>> {
    let current = allowed::global().unwrap_or_else(|| crate::permissions::kept(&shared.backend.store));
    let (sessions, folders) = recent(shared)?;
    let text = evidence(&tally(&sessions), &folders, &current);
    let models = shared.backend.client.models().map_err(ApiError::failed)?;
    let model = domain::model::choose_model(&models, None).ok_or_else(|| ApiError::bad(crate::models::none()))?;
    let cwd = folders.first().cloned().unwrap_or_else(|| PathBuf::from("."));
    let session = shared.backend.client.session(cwd, model);
    let found =
        session.discover_permissions(&text).and_then(app::Run::suggested_permissions).map_err(ApiError::failed)?;
    Ok(fresh(found, &current))
}

pub async fn discover(State(ctx): State<Ctx>) -> ApiResult<Response> {
    let shared = Arc::clone(&ctx);
    let found = crate::sessions::blocking(move || discovered(&shared)).await?;
    Ok(reply(json!({ "suggestions": found })))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tool(name: &str, input: Value) -> Entry {
        Entry::Tool {
            id: String::new(),
            name: name.into(),
            summary: String::new(),
            state: domain::conversation::ToolState::Done,
            output: None,
            input: Some(input),
        }
    }

    #[test]
    fn what_ran_is_counted_by_rule_across_sessions() {
        let browser = format!("mcp__{}__navigate", domain::mcp::browser_server());
        let one = vec![
            tool("Bash", json!({ "command": "cargo test -p x && git status" })),
            tool("Bash", json!({ "command": "cargo test" })),
            tool("WebFetch", json!({ "url": "https://docs.rs/serde" })),
            tool(&browser, json!({ "url": "https://www.gov.br/x" })),
            tool("mcp__atlassian__getJiraIssue", json!({})),
        ];
        let two = vec![tool("Bash", json!({ "command": "cargo test --workspace" }))];
        let seen = tally(&[("a".into(), one), ("b".into(), two)]);
        let shown: Vec<(String, usize, usize)> =
            seen.iter().map(|(grant, count)| (grant.rule(), count.times, count.sessions.len())).collect();
        assert_eq!(shown[0], ("Bash(cargo test:*)".to_owned(), 3, 2));
        let rules: Vec<&str> = shown.iter().map(|(rule, ..)| rule.as_str()).collect();
        for rule in ["Bash(git status:*)", "Site(docs.rs)", "Site(www.gov.br)", "mcp__atlassian__getJiraIssue"] {
            assert!(rules.contains(&rule), "{rule} in {rules:?}");
        }
    }

    #[test]
    fn what_the_list_already_covers_is_not_suggested() {
        let current = Allowed::of([Grant::Command("cargo test".into()), Grant::Site("docs.rs".into())]);
        let suggest = |grant: Grant| Suggested { grant, why: String::new() };
        let found = vec![
            suggest(Grant::Command("cargo test --workspace".into())),
            suggest(Grant::Command("bun test".into())),
            suggest(Grant::Site("docs.rs".into())),
            suggest(Grant::Site("*.docs.rs".into())),
        ];
        let kept: Vec<String> = fresh(found, &current).iter().map(|s| s.grant.rule()).collect();
        assert_eq!(kept, ["Bash(bun test:*)", "Site(*.docs.rs)"]);
    }

    #[test]
    fn a_project_shows_its_scripts_and_instructions() {
        let folder = tempfile::tempdir().expect("dir");
        std::fs::write(folder.path().join("package.json"), r#"{"scripts":{"test":"bun test"}}"#).expect("write");
        std::fs::write(folder.path().join("CLAUDE.md"), "Rode cargo nextest").expect("write");
        let text = project(folder.path());
        assert!(text.contains("\"test\":\"bun test\"") && text.contains("Rode cargo nextest"), "{text}");
        assert!(project(&folder.path().join("nada")).is_empty());
    }
}
