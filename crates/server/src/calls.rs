use connectors::contracts::{cloud, code, mcp, pipelines, tests as suite, tracker};
use domain::connection::{Capability, Connection, ConnectionId, ProviderKey};
use serde_json::{Value, json};
use store::ConnectionRecord;

use crate::attachments::random_id;
use crate::error::{ApiError, ApiResult, said};
use crate::marks::{iso, now_ms};
use crate::state::Shared;

pub(crate) fn connection(record: &ConnectionRecord) -> Connection {
    Connection {
        id: ConnectionId::from(record.id.as_str()),
        provider: ProviderKey::from(record.provider.as_str()),
        label: record.label.clone(),
        account: record.account.clone().unwrap_or_default(),
        capabilities: record.capabilities.iter().map(|name| Capability::from(name.as_str())).collect(),
    }
}

struct Caller<'a> {
    shared: &'a Shared,
    record: &'a ConnectionRecord,
}

impl Caller<'_> {
    async fn call(&self, action: &str, args: Value) -> ApiResult<Value> {
        let key = format!("{}:{}", brand::COMMAND, random_id()?);
        let hub = &self.shared.backend.connectors;
        Ok(hub.call(&connection(self.record), &self.record.settings, action, &args, &key).await?)
    }
}

fn text(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

fn person(found: &Value) -> Value {
    let name = match found {
        Value::Object(_) => text(&found["name"]),
        other => text(other),
    };
    let id = found.get("id").map(text).filter(|id| !id.is_empty()).unwrap_or_else(|| name.clone());
    json!({ "id": id, "name": name, "avatar": null })
}

fn arg<'a>(args: &'a Value, name: &str) -> ApiResult<&'a str> {
    args[name].as_str().filter(|text| !text.is_empty()).ok_or_else(|| {
        ApiError::bad(said(format!("{name} is missing."), format!("Falta {name}."), format!("Falta {name}.")))
    })
}

async fn repos(caller: &Caller<'_>, args: &Value) -> ApiResult<Value> {
    let mut wanted = json!({});
    if let Some(query) = args["query"].as_str().filter(|query| !query.is_empty()) {
        wanted["query"] = json!(query);
    }
    let found = caller.call(code::REPOS, wanted).await?;
    let listed: Vec<Value> = items(&found)
        .iter()
        .map(|repo| {
            json!({
                "id": text(&repo["id"]), "ref": text(&repo["ref"]), "name": text(&repo["name"]),
                "url": repo["url"].as_str(), "defaultBranch": repo["default_branch"].as_str(),
                "private": repo["private"].as_bool().unwrap_or(false),
            })
        })
        .filter(|repo| !text(&repo["ref"]).is_empty())
        .collect();
    Ok(json!(listed))
}

pub async fn run(shared: &Shared, record: &ConnectionRecord, action: &str, args: &Value) -> ApiResult<Value> {
    let caller = Caller { shared, record };
    match action {
        "tracker.search" => search(&caller, args).await,
        "tracker.projects" => projects(&caller, tracker::PROJECTS, args).await,
        "tests.projects" => projects(&caller, suite::PROJECTS, args).await,
        "security.projects" => projects(&caller, connectors::contracts::security::PROJECTS, args).await,
        "tracker.issue" => Ok(issue(&caller.call(tracker::ISSUE, json!({ "issue": arg(args, "key")? })).await?)),
        "tracker.comment" => {
            let body = arg(args, "body")?;
            let made = caller.call(tracker::COMMENT, json!({ "issue": arg(args, "key")?, "body": body })).await?;
            let you = json!({ "id": "you", "name": "Você", "avatar": null, "you": true });
            Ok(json!({ "id": text(&made["id"]), "author": you, "body": body, "at": iso(now_ms()), "kind": "comment" }))
        }
        "code.repos" => repos(&caller, args).await,
        "code.pulls" => Ok(pulls(&caller.call(code::PULL_REQUESTS, json!({ "repo": arg(args, "repo")? })).await?)),
        "code.branches" => Ok(branches(&caller.call(code::BRANCHES, json!({ "repo": arg(args, "repo")? })).await?)),
        "code.change" => change(&caller, arg(args, "change")?).await,
        "ci.run" => pipeline(&caller, arg(args, "run")?).await,
        "cloud.accounts_and_roles" => Ok(accounts(&caller.call(cloud::ACCOUNTS_AND_ROLES, json!({})).await?)),
        "mcp.tools" => caller.call(mcp::TOOLS, json!({})).await,
        action if is_options(action) => options(&caller, action, args).await,
        action if REPORTS.contains(&action) => caller.call(action, args.clone()).await,
        "changes.get" => {
            caller.call(connectors::contracts::changes::GET, json!({ "change": arg(args, "change")? })).await
        }
        _ => Err(ApiError::bad(said(
            format!("The action {action} does not exist for the page."),
            format!("La acción {action} no existe para la página."),
            format!("A ação {action} não existe para a página."),
        ))),
    }
}

pub fn pulls(raw: &Value) -> Value {
    let listed: Vec<Value> = items(raw)
        .iter()
        .map(|pull| {
            json!({
                "key": text(&pull["ref"]), "number": pull["number"].as_u64().unwrap_or(0), "title": text(&pull["title"]),
                "head": text(&pull["head"]), "base": text(&pull["base"]), "author": text(&pull["author"]),
                "url": pull["url"].as_str(), "draft": pull["draft"].as_bool().unwrap_or(false),
            })
        })
        .filter(|pull| !text(&pull["key"]).is_empty())
        .collect();
    json!(listed)
}

pub fn branches(raw: &Value) -> Value {
    let listed: Vec<Value> = items(raw)
        .iter()
        .map(|branch| json!({ "name": text(&branch["name"]), "default": branch["default"].as_bool().unwrap_or(false) }))
        .filter(|branch| !text(&branch["name"]).is_empty())
        .collect();
    json!(listed)
}

pub fn accounts(raw: &Value) -> Value {
    let listed: Vec<Value> = list(raw, "accounts")
        .iter()
        .map(|account| {
            let roles: Vec<String> = items(&account["roles"]).iter().map(text).filter(|role| !role.is_empty()).collect();
            json!({ "id": text(&account["id"]), "name": text(&account["name"]), "email": text(&account["email"]), "roles": roles })
        })
        .filter(|account| !text(&account["id"]).is_empty())
        .collect();
    json!(listed)
}

async fn search(caller: &Caller<'_>, args: &Value) -> ApiResult<Value> {
    let query = args["query"].as_str().unwrap_or_default();
    let mut wanted = json!({ "query": query, "limit": 20, "pick": true });
    if let Some(project) = args["project"].as_str().filter(|project| !project.is_empty()) {
        wanted["project"] = json!(project);
    }
    let found = caller.call(tracker::SEARCH, wanted).await?;
    let listed: Vec<Value> = list(&found, "issues")
        .iter()
        .map(|issue| {
            let mut listed = json!({ "key": text(&issue["ref"]), "title": text(&issue["title"]), "status": issue["status"].as_str() });
            if let Some(kind) = issue["type"].as_str() {
                listed["type"] = json!(kind);
            }
            if let Some(sprint) = issue["sprint"].as_bool() {
                listed["sprint"] = json!(sprint);
            }
            listed
        })
        .collect();
    Ok(json!(listed))
}

const REPORTS: [&str; 3] = [
    connectors::contracts::security::FINDINGS,
    connectors::contracts::security::STATUS,
    connectors::contracts::quality::REPORT,
];

fn is_options(action: &str) -> bool {
    action
        .split_once('.')
        .is_some_and(|(capability, name)| !capability.is_empty() && name == connectors::contracts::OPTIONS)
}

async fn options(caller: &Caller<'_>, action: &str, args: &Value) -> ApiResult<Value> {
    let mut wanted = json!({});
    if let Some(query) = args["query"].as_str().filter(|query| !query.is_empty()) {
        wanted["query"] = json!(query);
    }
    let found = caller.call(action, wanted).await?;
    let listed: Vec<Value> = items(&found)
        .iter()
        .map(|option| {
            let value = text(&option["value"]);
            let label = Some(text(&option["label"])).filter(|label| !label.is_empty()).unwrap_or_else(|| value.clone());
            json!({ "value": value, "label": label, "detail": option["detail"].as_str() })
        })
        .filter(|option| !text(&option["value"]).is_empty())
        .collect();
    Ok(json!(listed))
}

async fn projects(caller: &Caller<'_>, action: &str, args: &Value) -> ApiResult<Value> {
    let mut wanted = json!({});
    if let Some(query) = args["query"].as_str().filter(|query| !query.is_empty()) {
        wanted["query"] = json!(query);
    }
    let found = caller.call(action, wanted).await?;
    let listed: Vec<Value> = items(&found)
        .iter()
        .map(|project| {
            json!({
                "id": text(&project["id"]), "key": text(&project["key"]), "name": text(&project["name"]),
                "avatar": project["avatar"].as_str(),
            })
        })
        .filter(|project| !text(&project["key"]).is_empty())
        .collect();
    Ok(json!(listed))
}

fn list<'a>(value: &'a Value, name: &str) -> &'a [Value] {
    value[name].as_array().map(Vec::as_slice).unwrap_or_default()
}

fn items(value: &Value) -> &[Value] {
    value.as_array().map(Vec::as_slice).unwrap_or_default()
}

fn comment(raw: &Value, kind: &str) -> Value {
    let author = match raw["agent"].as_str().filter(|agent| !agent.is_empty()) {
        Some(agent) => json!({ "id": agent, "name": agent, "avatar": null, "agent": true }),
        None => person(&raw["author"]),
    };
    json!({ "id": text(&raw["id"]), "author": author, "body": text(&raw["body"]), "at": text(&raw["at"]), "kind": kind })
}

pub fn issue(raw: &Value) -> Value {
    let attachments: Vec<Value> = list(raw, "attachments")
        .iter()
        .map(|file| json!({ "name": text(&file["name"]), "size": text(&file["size"]), "url": file["url"].as_str() }))
        .collect();
    let subtasks: Vec<Value> = list(raw, "subtasks")
        .iter()
        .map(|task| {
            let done = task["done"].as_bool().unwrap_or(false);
            json!({
                "key": text(&task["ref"]), "title": text(&task["title"]), "done": done, "byAgent": false,
                "type": task["type"].as_str(),
            })
        })
        .collect();
    let comments: Vec<Value> = list(raw, "comments").iter().map(|found| comment(found, "comment")).collect();
    let assignee = if raw["assignee"].is_null() { Value::Null } else { person(&raw["assignee"]) };
    json!({
        "key": text(&raw["ref"]), "url": raw["url"].as_str(), "title": text(&raw["title"]),
        "status": raw["status"].as_str(), "type": raw["type"].as_str(), "parent": raw["parent"].as_str(),
        "assignee": assignee, "description": text(&raw["description"]),
        "attachments": attachments, "subtasks": subtasks, "comments": comments,
    })
}

async fn change(caller: &Caller<'_>, raw: &str) -> ApiResult<Value> {
    let args = json!({ "pr": raw });
    let pull = caller.call(code::PULL_REQUEST, args.clone()).await?;
    let files = caller.call(code::FILES_DIFF, args.clone()).await?;
    let threads = caller.call(code::REVIEW_COMMENTS, args).await?;
    Ok(change_data(raw, &pull, &files, &threads))
}

fn first_line(patch: &str) -> u64 {
    let header = patch.lines().next().unwrap_or_default();
    let added = header.split_whitespace().find(|part| part.starts_with('+')).unwrap_or("+1");
    added.trim_start_matches('+').split(',').next().and_then(|line| line.parse().ok()).unwrap_or(1)
}

fn review_kind(verdict: &Value) -> &'static str {
    match verdict.as_str() {
        Some("approve") => "approved",
        Some("request_changes") => "changes",
        _ => "comment",
    }
}

pub fn change_data(key: &str, pull: &Value, files: &Value, threads: &Value) -> Value {
    let files: Vec<Value> = items(files)
        .iter()
        .map(|file| {
            let patch = text(&file["patch"]);
            json!({
                "path": text(&file["path"]), "additions": file["additions"].as_u64().unwrap_or(0),
                "deletions": file["deletions"].as_u64().unwrap_or(0), "firstLine": first_line(&patch), "diff": patch,
            })
        })
        .collect();
    let mut comments: Vec<Value> = list(pull, "reviews")
        .iter()
        .filter(|review| !text(&review["body"]).is_empty() || review["verdict"] != "comment")
        .map(|review| comment(review, review_kind(&review["verdict"])))
        .collect();
    for found in items(threads) {
        let mut shown = comment(found, "comment");
        shown["file"] = found["path"].clone();
        shown["line"] = found["line"].clone();
        comments.push(shown);
    }
    let state = match pull["state"].as_str() {
        Some("merged") => "merged",
        Some("closed") => "closed",
        _ => "open",
    };
    json!({
        "key": key, "number": pull["number"].as_u64().unwrap_or(0), "title": text(&pull["title"]),
        "url": pull["url"].as_str(), "state": state, "author": person(&pull["author"]),
        "head": text(&pull["head"]), "base": text(&pull["base"]), "body": text(&pull["body"]),
        "files": files, "comments": comments,
    })
}

async fn pipeline(caller: &Caller<'_>, raw: &str) -> ApiResult<Value> {
    let found = caller.call(pipelines::RUN, json!({ "run": raw })).await?;
    Ok(pipeline_run(raw, &found))
}

fn shown_state(state: &Value) -> &'static str {
    match state.as_str() {
        Some("passed") => "passed",
        Some("failed" | "cancelled") => "failed",
        Some("skipped") => "skipped",
        Some("waiting") => "waiting",
        Some("running") => "running",
        _ => "queued",
    }
}

fn command_state(state: &Value) -> &'static str {
    match shown_state(state) {
        "passed" | "skipped" => "done",
        "failed" => "failed",
        "running" => "running",
        _ => "todo",
    }
}

fn seconds(started: &Value, finished: &Value) -> Value {
    let parse = |value: &Value| value.as_str().and_then(|text| text.parse::<jiff::Timestamp>().ok());
    match (parse(started), parse(finished)) {
        (Some(start), Some(end)) => json!((end.as_second() - start.as_second()).max(0)),
        _ => Value::Null,
    }
}

fn staged(jobs: &mut [Value]) {
    for _ in 0..jobs.len() {
        let stages: Vec<(String, u64)> =
            jobs.iter().map(|job| (text(&job["id"]), job["stage"].as_u64().unwrap_or(0))).collect();
        for job in jobs.iter_mut() {
            let needs = job["needs"].as_array().cloned().unwrap_or_default();
            let deepest = needs
                .iter()
                .filter_map(|need| {
                    stages.iter().find(|(id, _)| Some(id.as_str()) == need.as_str()).map(|(_, at)| at + 1)
                })
                .max()
                .unwrap_or(0);
            job["stage"] = json!(deepest);
        }
    }
}

fn job(found: &Value) -> Value {
    let steps: Vec<Value> = list(found, "steps")
        .iter()
        .map(|step| {
            json!({
                "name": text(&step["name"]), "state": command_state(&step["state"]),
                "seconds": seconds(&step["started"], &step["finished"]), "log": "",
            })
        })
        .collect();
    let id = found["key"].as_str().map_or_else(|| text(&found["id"]), str::to_owned);
    json!({
        "id": id, "name": text(&found["name"]), "stage": 0, "needs": found["needs"].as_array().cloned().unwrap_or_default(),
        "state": shown_state(&found["state"]), "seconds": seconds(&found["started"], &found["finished"]),
        "condition": null, "gate": null, "waitingSince": null, "steps": steps,
    })
}

pub fn pipeline_run(key: &str, raw: &Value) -> Value {
    let mut jobs: Vec<Value> = list(raw, "jobs").iter().map(job).collect();
    staged(&mut jobs);
    json!({
        "id": key, "title": text(&raw["pipeline"]), "file": text(&raw["file"]),
        "number": raw["attempt"].as_u64().unwrap_or(1), "event": text(&raw["event"]), "ref": text(&raw["head"]),
        "url": raw["url"].as_str(), "state": shown_state(&raw["state"]), "jobs": jobs,
    })
}

#[cfg(test)]
#[path = "calls_tests.rs"]
mod tests;
