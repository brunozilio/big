use serde_json::{Value, json};

use super::{Api, repo};
use crate::error::{ActionError, ErrorClass};
use crate::http::Method;
use crate::idempotency;
use crate::provider::{Args, text_of};

const FILE_PAGES: usize = 30;

const THREADS: &str = "query($owner:String!,$name:String!,$number:Int!){repository(owner:$owner,name:$name){pullRequest(number:$number){reviewThreads(first:100){nodes{id isResolved isOutdated path line comments(first:50){nodes{id body createdAt author{login}}}}}}}}";
const RESOLVE: &str = "mutation($thread:ID!){resolveReviewThread(input:{threadId:$thread}){thread{id isResolved}}}";

pub fn pull_summary(pull: &Value, created: bool) -> Value {
    let merged = pull["merged"].as_bool().unwrap_or(false) || !pull["merged_at"].is_null();
    json!({
        "number": pull["number"],
        "url": text_of(pull, "/html_url"),
        "title": text_of(pull, "/title"),
        "state": if merged { "merged".to_owned() } else { text_of(pull, "/state") },
        "merged": merged,
        "draft": pull["draft"].as_bool().unwrap_or(false),
        "head": text_of(pull, "/head/ref"),
        "base": text_of(pull, "/base/ref"),
        "sha": text_of(pull, "/head/sha"),
        "mergeable": pull["mergeable"],
        "created": created,
    })
}

async fn by_branch(api: &Api<'_>, repo: &str, head: &str) -> Result<Option<Value>, ActionError> {
    let owner = repo.split('/').next().unwrap_or_default();
    let head: String = url::form_urlencoded::byte_serialize(format!("{owner}:{head}").as_bytes()).collect();
    let found = api.get(&format!("/repos/{repo}/pulls?head={head}&state=all&per_page=20")).await?;
    let pulls = found.as_array().map(Vec::as_slice).unwrap_or_default();
    let open = pulls.iter().find(|pull| pull["state"] == "open");
    let merged = pulls.iter().find(|pull| !pull["merged_at"].is_null());
    Ok(open.or(merged).cloned())
}

pub async fn open_pull_request(api: &Api<'_>, args: Args<'_>, key: &str) -> Result<Value, ActionError> {
    let repo = repo(args)?;
    let head = args.text("head")?;
    if let Some(found) = by_branch(api, repo, head).await? {
        return Ok(pull_summary(&found, false));
    }
    let body = json!({
        "title": args.text("title")?,
        "body": idempotency::marked(args.optional_text("body").unwrap_or_default(), key),
        "head": head,
        "base": args.text("base")?,
        "draft": args.0.get("draft").and_then(Value::as_bool).unwrap_or(false),
    });
    match api.send(Method::Post, &format!("/repos/{repo}/pulls"), &body).await {
        Ok(pull) => Ok(pull_summary(&pull, true)),
        Err(error) if error.class == ErrorClass::Permanent && error.message.contains("already exists") => {
            let found = by_branch(api, repo, head).await?.ok_or(error)?;
            Ok(pull_summary(&found, false))
        }
        Err(error) => Err(error),
    }
}

fn checks_state(runs: &[Value]) -> &'static str {
    if runs.is_empty() {
        return "none";
    }
    let failed = ["failure", "timed_out", "cancelled", "action_required", "startup_failure"];
    if runs.iter().any(|run| failed.contains(&run["conclusion"].as_str().unwrap_or_default())) {
        "failure"
    } else if runs.iter().any(|run| run["status"] != "completed") {
        "pending"
    } else {
        "success"
    }
}

pub async fn pull_request(api: &Api<'_>, args: Args<'_>) -> Result<Value, ActionError> {
    let repo = repo(args)?;
    let number = args.number("number")?;
    let pull = api.get(&format!("/repos/{repo}/pulls/{number}")).await?;
    let sha = text_of(&pull, "/head/sha");
    let checks = api.get(&format!("/repos/{repo}/commits/{sha}/check-runs?per_page=100")).await?;
    let reviews = api.get(&format!("/repos/{repo}/pulls/{number}/reviews?per_page=100")).await?;
    let runs = checks["check_runs"].as_array().map(Vec::as_slice).unwrap_or_default();
    let listed: Vec<Value> = runs
        .iter()
        .map(|run| json!({ "name": run["name"], "status": run["status"], "conclusion": run["conclusion"], "url": run["html_url"] }))
        .collect();
    let reviews: Vec<Value> = reviews
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .map(|review| {
            json!({
                "id": review["id"],
                "user": text_of(review, "/user/login"),
                "state": review["state"],
                "body": review["body"],
                "at": review["submitted_at"],
            })
        })
        .collect();
    let mut summary = pull_summary(&pull, false);
    summary["checks"] = json!({ "state": checks_state(runs), "runs": listed });
    summary["reviews"] = json!(reviews);
    Ok(summary)
}

pub async fn files_diff(api: &Api<'_>, args: Args<'_>) -> Result<Value, ActionError> {
    let repo = repo(args)?;
    let number = args.number("number")?;
    let mut files = Vec::new();
    for page in 1..=FILE_PAGES {
        let listed = api.get(&format!("/repos/{repo}/pulls/{number}/files?per_page=100&page={page}")).await?;
        let listed = listed.as_array().cloned().unwrap_or_default();
        let last = listed.len() < 100;
        files.extend(listed.iter().map(|file| {
            json!({
                "path": file["filename"],
                "previous_path": file["previous_filename"],
                "status": file["status"],
                "additions": file["additions"],
                "deletions": file["deletions"],
                "patch": file["patch"],
            })
        }));
        if last {
            break;
        }
    }
    Ok(json!({ "files": files }))
}

pub async fn review_comments(api: &Api<'_>, args: Args<'_>) -> Result<Value, ActionError> {
    let repo = repo(args)?;
    let number = args.number("number")?;
    let (owner, name) = repo.split_once('/').unwrap_or_default();
    let data = api.graphql(THREADS, json!({ "owner": owner, "name": name, "number": number })).await?;
    let nodes = data
        .pointer("/repository/pullRequest/reviewThreads/nodes")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default();
    let threads: Vec<Value> = nodes
        .iter()
        .map(|thread| {
            let comments: Vec<Value> = thread
                .pointer("/comments/nodes")
                .and_then(Value::as_array)
                .map(Vec::as_slice)
                .unwrap_or_default()
                .iter()
                .map(|comment| {
                    json!({
                        "id": comment["id"],
                        "author": text_of(comment, "/author/login"),
                        "body": comment["body"],
                        "at": comment["createdAt"],
                    })
                })
                .collect();
            json!({
                "id": thread["id"],
                "resolved": thread["isResolved"],
                "outdated": thread["isOutdated"],
                "path": thread["path"],
                "line": thread["line"],
                "comments": comments,
            })
        })
        .collect();
    Ok(json!({ "threads": threads }))
}

fn review_event(event: &str) -> Result<&'static str, ActionError> {
    match event {
        "approve" => Ok("APPROVE"),
        "comment" => Ok("COMMENT"),
        "request_changes" => Ok("REQUEST_CHANGES"),
        other => Err(ActionError::permanent(format!("{other:?} is not approve, comment or request_changes"))),
    }
}

pub async fn submit_review(api: &Api<'_>, args: Args<'_>, key: &str) -> Result<Value, ActionError> {
    let repo = repo(args)?;
    let number = args.number("number")?;
    let event = review_event(args.text("event")?)?;
    if !key.is_empty() {
        let reviews = api.get(&format!("/repos/{repo}/pulls/{number}/reviews?per_page=100")).await?;
        let listed = reviews.as_array().map(Vec::as_slice).unwrap_or_default();
        if let Some(found) =
            listed.iter().find(|review| idempotency::carries(review["body"].as_str().unwrap_or_default(), key))
        {
            return Ok(json!({ "id": found["id"], "state": found["state"], "created": false }));
        }
    }
    let comments: Vec<Value> = args
        .list("comments")
        .iter()
        .map(|comment| json!({ "path": comment["path"], "line": comment["line"], "body": comment["body"], "side": "RIGHT" }))
        .collect();
    let mut body =
        json!({ "event": event, "body": idempotency::marked(args.optional_text("body").unwrap_or_default(), key) });
    if !comments.is_empty() {
        body["comments"] = json!(comments);
    }
    let review = api.send(Method::Post, &format!("/repos/{repo}/pulls/{number}/reviews"), &body).await?;
    Ok(json!({ "id": review["id"], "state": review["state"], "created": true }))
}

pub async fn resolve_thread(api: &Api<'_>, args: Args<'_>) -> Result<Value, ActionError> {
    let thread = args.text("thread")?;
    let data = api.graphql(RESOLVE, json!({ "thread": thread })).await?;
    Ok(json!({ "resolved": data.pointer("/resolveReviewThread/thread/isResolved").cloned().unwrap_or(json!(true)) }))
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
    let repo = repo(args)?;
    let number = args.number("number")?;
    let method = merge_method(args.optional_text("method"))?;
    let pull = api.get(&format!("/repos/{repo}/pulls/{number}")).await?;
    if pull["merged"].as_bool().unwrap_or(false) || !pull["merged_at"].is_null() {
        return Ok(json!({ "merged": true, "already": true, "sha": pull["merge_commit_sha"] }));
    }
    if pull["state"] == "closed" {
        return Err(ActionError::permanent(format!("{repo}#{number} was closed without merging")));
    }
    let body = json!({ "merge_method": method, "sha": text_of(&pull, "/head/sha") });
    let merged = api.send(Method::Put, &format!("/repos/{repo}/pulls/{number}/merge"), &body).await?;
    Ok(json!({ "merged": merged["merged"].as_bool().unwrap_or(true), "already": false, "sha": merged["sha"] }))
}
