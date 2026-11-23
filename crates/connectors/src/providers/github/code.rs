use serde_json::{Value, json};

use super::Api;
use super::refs::{pull_of, pull_ref, repo_of};
use super::states::{change, state, verdict};
use crate::error::{ActionError, ErrorClass};
use crate::http::Method;
use crate::idempotency;
use crate::provider::{Args, text_of};

const FILE_PAGES: usize = 30;

const THREADS: &str = "query($owner:String!,$name:String!,$number:Int!){repository(owner:$owner,name:$name){pullRequest(number:$number){reviewThreads(first:100){nodes{id isResolved isOutdated path line comments(first:50){nodes{id body createdAt author{login}}}}}}}}";
const RESOLVE: &str = "mutation($thread:ID!){resolveReviewThread(input:{threadId:$thread}){thread{id isResolved}}}";

fn list(value: &Value) -> &[Value] {
    value.as_array().map(Vec::as_slice).unwrap_or_default()
}

fn merged(pull: &Value) -> bool {
    pull["merged"].as_bool().unwrap_or(false) || !pull["merged_at"].is_null()
}

pub fn pull_summary(repo: &str, pull: &Value, created: bool) -> Value {
    let number = pull["number"].as_u64().unwrap_or_default();
    let merged = merged(pull);
    json!({
        "ref": pull_ref(repo, number),
        "repo": repo,
        "number": number,
        "url": text_of(pull, "/html_url"),
        "title": text_of(pull, "/title"),
        "body": text_of(pull, "/body"),
        "state": if merged { "merged".to_owned() } else { text_of(pull, "/state") },
        "merged": merged,
        "draft": pull["draft"].as_bool().unwrap_or(false),
        "head": text_of(pull, "/head/ref"),
        "base": text_of(pull, "/base/ref"),
        "sha": text_of(pull, "/head/sha"),
        "mergeable": pull["mergeable"],
        "author": text_of(pull, "/user/login"),
        "updated": text_of(pull, "/updated_at"),
        "checks": [],
        "reviews": [],
        "created": created,
    })
}

async fn by_branch(api: &Api<'_>, repo: &str, head: &str) -> Result<Option<Value>, ActionError> {
    let owner = repo.split('/').next().unwrap_or_default();
    let head: String = url::form_urlencoded::byte_serialize(format!("{owner}:{head}").as_bytes()).collect();
    let found = api.get(&format!("/repos/{repo}/pulls?head={head}&state=all&per_page=20")).await?;
    let pulls = list(&found);
    let open = pulls.iter().find(|pull| pull["state"] == "open");
    let merged = pulls.iter().find(|pull| !pull["merged_at"].is_null());
    Ok(open.or(merged).cloned())
}

async fn base_of(api: &Api<'_>, args: Args<'_>, repo: &str) -> Result<String, ActionError> {
    if let Some(base) = args.optional_text("base").or_else(|| api.creds.setting("base")) {
        return Ok(base.to_owned());
    }
    let found = api.get(&format!("/repos/{repo}")).await?;
    Ok(found["default_branch"].as_str().filter(|name| !name.is_empty()).unwrap_or("main").to_owned())
}

pub async fn open_pull_request(api: &Api<'_>, args: Args<'_>, key: &str) -> Result<Value, ActionError> {
    let repo = repo_of(api, args)?;
    let head = args.text("head")?;
    if let Some(found) = by_branch(api, &repo, head).await? {
        return Ok(pull_summary(&repo, &found, false));
    }
    let body = json!({
        "title": args.text("title")?,
        "body": idempotency::marked(args.optional_text("body").unwrap_or_default(), key),
        "head": head,
        "base": base_of(api, args, &repo).await?,
        "draft": args.0.get("draft").and_then(Value::as_bool).unwrap_or(false),
    });
    match api.send(Method::Post, &format!("/repos/{repo}/pulls"), &body).await {
        Ok(pull) => Ok(pull_summary(&repo, &pull, true)),
        Err(error) if error.class == ErrorClass::Permanent && error.message.contains("already exists") => {
            let found = by_branch(api, &repo, head).await?.ok_or(error)?;
            Ok(pull_summary(&repo, &found, false))
        }
        Err(error) => Err(error),
    }
}

fn check(run: &Value) -> Value {
    json!({ "name": run["name"], "state": state(&run["status"], &run["conclusion"]), "url": run["html_url"] })
}

fn review(found: &Value) -> Value {
    json!({
        "id": found["id"].to_string(),
        "author": text_of(found, "/user/login"),
        "verdict": verdict(&found["state"]),
        "body": text_of(found, "/body"),
        "at": text_of(found, "/submitted_at"),
    })
}

pub async fn pull_request(api: &Api<'_>, args: Args<'_>) -> Result<Value, ActionError> {
    let (repo, number) = pull_of(api, args)?;
    let pull = api.get(&format!("/repos/{repo}/pulls/{number}")).await?;
    let sha = text_of(&pull, "/head/sha");
    let checks = api.get(&format!("/repos/{repo}/commits/{sha}/check-runs?per_page=100")).await?;
    let reviews = api.get(&format!("/repos/{repo}/pulls/{number}/reviews?per_page=100")).await?;
    let mut summary = pull_summary(&repo, &pull, false);
    summary["checks"] = Value::Array(list(&checks["check_runs"]).iter().map(check).collect());
    summary["reviews"] = Value::Array(list(&reviews).iter().map(review).collect());
    Ok(summary)
}

fn file(found: &Value) -> Value {
    json!({
        "path": found["filename"],
        "previous": found["previous_filename"],
        "change": change(&found["status"]),
        "additions": found["additions"].as_u64().unwrap_or(0),
        "deletions": found["deletions"].as_u64().unwrap_or(0),
        "patch": text_of(found, "/patch"),
    })
}

pub async fn files_diff(api: &Api<'_>, args: Args<'_>) -> Result<Value, ActionError> {
    let (repo, number) = pull_of(api, args)?;
    let mut files = Vec::new();
    for page in 1..=FILE_PAGES {
        let listed = api.get(&format!("/repos/{repo}/pulls/{number}/files?per_page=100&page={page}")).await?;
        let listed = list(&listed);
        files.extend(listed.iter().map(file));
        if listed.len() < 100 {
            break;
        }
    }
    Ok(Value::Array(files))
}

fn thread_comments(thread: &Value) -> Vec<Value> {
    let comments = thread.pointer("/comments/nodes").map(list).unwrap_or_default();
    comments
        .iter()
        .map(|comment| {
            json!({
                "id": comment["id"],
                "thread": thread["id"],
                "path": thread["path"],
                "line": thread["line"],
                "author": text_of(comment, "/author/login"),
                "body": text_of(comment, "/body"),
                "resolved": thread["isResolved"].as_bool().unwrap_or(false),
                "outdated": thread["isOutdated"].as_bool().unwrap_or(false),
                "at": text_of(comment, "/createdAt"),
            })
        })
        .collect()
}

pub async fn review_comments(api: &Api<'_>, args: Args<'_>) -> Result<Value, ActionError> {
    let (repo, number) = pull_of(api, args)?;
    let (owner, name) = repo.split_once('/').unwrap_or_default();
    let data = api.graphql(THREADS, json!({ "owner": owner, "name": name, "number": number })).await?;
    let nodes = data.pointer("/repository/pullRequest/reviewThreads/nodes").map(list).unwrap_or_default();
    Ok(Value::Array(nodes.iter().flat_map(thread_comments).collect()))
}

fn review_event(verdict: &str) -> Result<&'static str, ActionError> {
    match verdict {
        "approve" => Ok("APPROVE"),
        "comment" => Ok("COMMENT"),
        "request_changes" => Ok("REQUEST_CHANGES"),
        other => Err(ActionError::permanent(format!("{other:?} is not approve, comment or request_changes"))),
    }
}

pub async fn submit_review(api: &Api<'_>, args: Args<'_>, key: &str) -> Result<Value, ActionError> {
    let (repo, number) = pull_of(api, args)?;
    let chosen = args.text("verdict")?;
    let event = review_event(chosen)?;
    if !key.is_empty() {
        let reviews = api.get(&format!("/repos/{repo}/pulls/{number}/reviews?per_page=100")).await?;
        if let Some(found) =
            list(&reviews).iter().find(|review| idempotency::carries(review["body"].as_str().unwrap_or_default(), key))
        {
            return Ok(json!({ "id": found["id"].to_string(), "verdict": verdict(&found["state"]), "created": false }));
        }
    }
    let comments: Vec<Value> = args
        .list("lines")
        .iter()
        .map(|line| json!({ "path": line["path"], "line": line["line"], "body": line["body"], "side": "RIGHT" }))
        .collect();
    let mut body =
        json!({ "event": event, "body": idempotency::marked(args.optional_text("body").unwrap_or_default(), key) });
    if !comments.is_empty() {
        body["comments"] = json!(comments);
    }
    let made = api.send(Method::Post, &format!("/repos/{repo}/pulls/{number}/reviews"), &body).await?;
    Ok(json!({ "id": made["id"].to_string(), "verdict": chosen, "created": true }))
}

pub async fn resolve_thread(api: &Api<'_>, args: Args<'_>) -> Result<Value, ActionError> {
    let thread = args.text("thread")?;
    let data = api.graphql(RESOLVE, json!({ "thread": thread })).await?;
    let resolved = data.pointer("/resolveReviewThread/thread/isResolved").and_then(Value::as_bool).unwrap_or(true);
    Ok(json!({ "thread": thread, "resolved": resolved }))
}

fn merge_method(method: Option<&str>) -> Result<&'static str, ActionError> {
    match method.unwrap_or("merge") {
        "merge" => Ok("merge"),
        "squash" => Ok("squash"),
        "rebase" => Ok("rebase"),
        other => Err(ActionError::permanent(format!("{other:?} is not merge, squash or rebase"))),
    }
}

pub async fn merge(api: &Api<'_>, args: Args<'_>) -> Result<Value, ActionError> {
    let (repo, number) = pull_of(api, args)?;
    let method = merge_method(args.optional_text("method"))?;
    let pull = api.get(&format!("/repos/{repo}/pulls/{number}")).await?;
    if merged(&pull) {
        return Ok(json!({ "merged": true, "already": true, "sha": pull["merge_commit_sha"] }));
    }
    if pull["state"] == "closed" {
        return Err(ActionError::permanent(format!("{repo}#{number} was closed without merging")));
    }
    let body = json!({ "merge_method": method, "sha": text_of(&pull, "/head/sha") });
    let done = api.send(Method::Put, &format!("/repos/{repo}/pulls/{number}/merge"), &body).await?;
    Ok(json!({ "merged": done["merged"].as_bool().unwrap_or(true), "already": false, "sha": done["sha"] }))
}
