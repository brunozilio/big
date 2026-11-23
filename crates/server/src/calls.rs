use connectors::contracts::{code, pipelines, tracker};
use domain::connection::{Capability, Connection, ConnectionId, ProviderKey};
use serde_json::{Value, json};
use store::ConnectionRecord;

use crate::attachments::random_id;
use crate::error::{ApiError, ApiResult};
use crate::marks::{iso, now_ms};
use crate::state::Shared;

fn connection(record: &ConnectionRecord) -> Connection {
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

fn person(name: &str) -> Value {
    json!({ "id": name, "name": name, "avatar": null })
}

fn arg<'a>(args: &'a Value, name: &str) -> ApiResult<&'a str> {
    args[name].as_str().filter(|text| !text.is_empty()).ok_or_else(|| ApiError::bad(format!("Falta {name}.")))
}

pub fn reference(raw: &str, settings: &Value) -> ApiResult<(String, u64)> {
    let (repo, number) = match raw.rsplit_once('#') {
        Some((repo, number)) => (repo.to_owned(), number),
        None => (settings["repo"].as_str().unwrap_or_default().to_owned(), raw),
    };
    let number =
        number.trim().parse::<u64>().map_err(|_| ApiError::bad(format!("{raw} não é uma referência válida.")))?;
    if repo.is_empty() {
        return Err(ApiError::bad(format!("{raw} não diz o repositório.")));
    }
    Ok((repo, number))
}

pub async fn run(shared: &Shared, record: &ConnectionRecord, action: &str, args: &Value) -> ApiResult<Value> {
    let caller = Caller { shared, record };
    match action {
        "tracker.search" => search(&caller, args).await,
        "tracker.issue" => Ok(issue(&caller.call(tracker::ISSUE, json!({ "issue": arg(args, "key")? })).await?)),
        "tracker.comment" => {
            let body = arg(args, "body")?;
            let made = caller.call(tracker::COMMENT, json!({ "issue": arg(args, "key")?, "body": body })).await?;
            let you = json!({ "id": "you", "name": "Você", "avatar": null, "you": true });
            Ok(json!({ "id": text(&made["id"]), "author": you, "body": body, "at": iso(now_ms()), "kind": "comment" }))
        }
        "code.change" => change(&caller, arg(args, "change")?).await,
        "ci.run" => pipeline(&caller, arg(args, "run")?).await,
        _ => Err(ApiError::bad(format!("A ação {action} não existe para a página."))),
    }
}

async fn search(caller: &Caller<'_>, args: &Value) -> ApiResult<Value> {
    let query = args["query"].as_str().unwrap_or_default();
    let found = caller.call(tracker::SEARCH, json!({ "text": query, "limit": 20 })).await?;
    let issues = found["issues"].as_array().map(Vec::as_slice).unwrap_or_default();
    let listed: Vec<Value> = issues
        .iter()
        .map(|issue| json!({ "key": text(&issue["key"]), "title": text(&issue["title"]), "status": issue["status"].as_str() }))
        .collect();
    Ok(json!(listed))
}

fn list<'a>(value: &'a Value, name: &str) -> &'a [Value] {
    value[name].as_array().map(Vec::as_slice).unwrap_or_default()
}

fn comment(raw: &Value, kind: &str) -> Value {
    let author = text(&raw["author"]);
    json!({ "id": text(&raw["id"]), "author": person(&author), "body": text(&raw["body"]), "at": text(&raw["at"]), "kind": kind })
}

pub fn issue(raw: &Value) -> Value {
    let attachments: Vec<Value> = list(raw, "attachments")
        .iter()
        .map(|file| json!({ "name": text(&file["name"]), "size": text(&file["size"]), "url": file["url"].as_str() }))
        .collect();
    let subtasks: Vec<Value> = list(raw, "subtasks")
        .iter()
        .map(|task| {
            let done = matches!(task["status"].as_str(), Some("Done" | "Feito" | "Concluído" | "Closed"));
            json!({ "key": text(&task["key"]), "title": text(&task["title"]), "done": done, "byAgent": false })
        })
        .collect();
    let comments: Vec<Value> = list(raw, "comments").iter().map(|found| comment(found, "comment")).collect();
    let assignee = raw["assignee"].as_str().map(person);
    json!({
        "key": text(&raw["key"]), "url": raw["url"].as_str(), "title": text(&raw["title"]),
        "status": raw["status"].as_str(), "assignee": assignee, "description": text(&raw["description"]),
        "attachments": attachments, "subtasks": subtasks, "comments": comments,
    })
}

async fn change(caller: &Caller<'_>, raw: &str) -> ApiResult<Value> {
    let (repo, number) = reference(raw, &caller.record.settings)?;
    let args = json!({ "repo": repo, "number": number });
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

pub fn change_data(key: &str, pull: &Value, files: &Value, threads: &Value) -> Value {
    let files: Vec<Value> = list(files, "files")
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
        .filter(|review| !text(&review["body"]).is_empty() || review["state"] != "COMMENTED")
        .map(|review| {
            let kind = match review["state"].as_str() {
                Some("APPROVED") => "approved",
                Some("CHANGES_REQUESTED") => "changes",
                _ => "comment",
            };
            let raw =
                json!({ "id": review["id"], "author": review["user"], "body": review["body"], "at": review["at"] });
            comment(&raw, kind)
        })
        .collect();
    for thread in list(threads, "threads") {
        for found in list(thread, "comments") {
            let mut shown = comment(found, "comment");
            shown["file"] = thread["path"].clone();
            shown["line"] = thread["line"].clone();
            comments.push(shown);
        }
    }
    let state = match pull["state"].as_str() {
        Some("merged") => "merged",
        Some("closed") => "closed",
        _ => "open",
    };
    json!({
        "key": key, "number": pull["number"].as_u64().unwrap_or(0), "title": text(&pull["title"]),
        "url": pull["url"].as_str(), "state": state, "author": person(&text(&pull["author"])),
        "head": text(&pull["head"]), "base": text(&pull["base"]), "body": text(&pull["body"]),
        "files": files, "comments": comments,
    })
}

async fn pipeline(caller: &Caller<'_>, raw: &str) -> ApiResult<Value> {
    let (repo, run) = reference(raw, &caller.record.settings)?;
    let found = caller.call(pipelines::RUN, json!({ "repo": repo, "run": run })).await?;
    Ok(pipeline_run(raw, &found))
}

fn job_state(status: &Value, conclusion: &Value) -> &'static str {
    match (status.as_str(), conclusion.as_str()) {
        (_, Some("success")) => "passed",
        (_, Some("failure" | "timed_out" | "cancelled" | "startup_failure")) => "failed",
        (_, Some("skipped" | "neutral")) => "skipped",
        (Some("waiting"), _) => "waiting",
        (Some("in_progress"), _) => "running",
        _ => "queued",
    }
}

fn seconds(started: &Value, completed: &Value) -> Value {
    let parse = |value: &Value| value.as_str().and_then(|text| text.parse::<jiff::Timestamp>().ok());
    match (parse(started), parse(completed)) {
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

pub fn pipeline_run(key: &str, raw: &Value) -> Value {
    let mut jobs: Vec<Value> = list(raw, "jobs")
        .iter()
        .map(|job| {
            let state = job_state(&job["status"], &job["conclusion"]);
            let steps: Vec<Value> = list(job, "steps")
                .iter()
                .map(|step| {
                    let state = match job_state(&step["status"], &step["conclusion"]) {
                        "passed" | "skipped" => "done",
                        "failed" => "failed",
                        "running" => "running",
                        _ => "todo",
                    };
                    json!({ "name": text(&step["name"]), "state": state, "seconds": null, "log": "" })
                })
                .collect();
            let id = job["key"].as_str().map_or_else(|| text(&job["id"]), str::to_owned);
            json!({
                "id": id, "name": text(&job["name"]), "stage": 0, "needs": job["needs"].as_array().cloned().unwrap_or_default(),
                "state": state, "seconds": seconds(&job["started"], &job["completed"]), "condition": null,
                "gate": null, "waitingSince": null, "steps": steps,
            })
        })
        .collect();
    staged(&mut jobs);
    json!({
        "id": key, "title": text(&raw["name"]), "file": text(&raw["workflow"]),
        "number": raw["attempt"].as_u64().unwrap_or(1), "event": text(&raw["event"]), "ref": text(&raw["ref"]),
        "url": raw["url"].as_str(), "state": job_state(&raw["status"], &raw["conclusion"]), "jobs": jobs,
    })
}

#[cfg(test)]
#[path = "calls_tests.rs"]
mod tests;
