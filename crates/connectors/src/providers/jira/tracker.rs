use serde_json::{Map, Value, json};

use super::adf::{adf_to_markdown, markdown_to_adf};
use super::{Api, jql_key, jql_text, listed, visible_projects};
use crate::error::ActionError;
use crate::idempotency;
use crate::provider::{Args, text_of};

const ISSUE_FIELDS: [&str; 11] = [
    "summary",
    "description",
    "status",
    "issuetype",
    "assignee",
    "labels",
    "attachment",
    "subtasks",
    "comment",
    "created",
    "updated",
];
const SUMMARY_FIELDS: [&str; 2] = ["summary", "status"];
const DEFAULT_LIMIT: u64 = 50;

pub fn markdown(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Null => String::new(),
        other => adf_to_markdown(other),
    }
}

pub fn person(value: &Value) -> Value {
    let name = text_of(value, "/displayName");
    if name.is_empty() {
        return Value::Null;
    }
    let id = text_of(value, "/accountId");
    json!({ "id": if id.is_empty() { name.clone() } else { id }, "name": name })
}

fn flagged(value: Option<&Value>) -> bool {
    value.and_then(Value::as_array).is_some_and(|flags| !flags.is_empty())
}

fn summary(api: &Api<'_>, issue: &Value) -> Value {
    let key = text_of(issue, "/key");
    json!({
        "ref": key,
        "title": text_of(issue, "/fields/summary"),
        "status": text_of(issue, "/fields/status/name"),
        "url": api.browse(&key),
    })
}

async fn find_issue(api: &Api<'_>, project: &str, key: &str) -> Result<Option<Value>, ActionError> {
    let jql = format!("project = \"{project}\" AND labels = \"{}\" ORDER BY created DESC", idempotency::label(key));
    let page = api.search(&jql, &SUMMARY_FIELDS, None).await?;
    Ok(listed(&page, "issues").first().cloned())
}

pub async fn create_issue(api: &Api<'_>, args: Args<'_>, key: &str) -> Result<Value, ActionError> {
    let project = api.project(args)?;
    let title = args.text("title")?;
    if !key.is_empty()
        && let Some(found) = find_issue(api, project, key).await?
    {
        let mut answer = summary(api, &found);
        answer["created"] = json!(false);
        return Ok(answer);
    }
    let parent = args.optional_text("parent");
    let kind = args.optional_text("type").unwrap_or(if parent.is_some() { "Subtask" } else { "Task" });
    let mut labels: Vec<Value> = args.list("labels").to_vec();
    if !key.is_empty() {
        labels.push(json!(idempotency::label(key)));
    }
    let mut arguments = json!({
        "projectKey": project,
        "summary": title,
        "issueTypeName": kind,
        "description": markdown_to_adf(args.optional_text("description").unwrap_or_default()),
        "contentFormat": "adf",
    });
    if let Some(parent) = parent {
        arguments["parent"] = json!(parent);
    }
    if !labels.is_empty() {
        arguments["additional_fields"] = json!({ "labels": labels });
    }
    let answer = api.tool("createJiraIssue", arguments).await?;
    let mut made = summary(api, &answer);
    made["title"] = json!(title);
    made["created"] = json!(true);
    Ok(made)
}

pub fn comment_of(comment: &Value) -> Value {
    json!({
        "id": text_of(comment, "/id"),
        "author": person(&comment["author"]),
        "body": markdown(&comment["body"]),
        "at": text_of(comment, "/created"),
    })
}

fn comments(fields: &Value) -> Vec<Value> {
    listed(&fields["comment"], "comments").iter().map(comment_of).collect()
}

async fn read(api: &Api<'_>, issue: &str, fields: &[&str]) -> Result<Value, ActionError> {
    let arguments = json!({ "issueIdOrKey": jql_key(issue)?, "fields": fields, "responseContentFormat": "markdown" });
    api.tool("getJiraIssue", arguments).await
}

fn subtask(api: &Api<'_>, task: &Value) -> Value {
    let mut found = summary(api, task);
    found["done"] = json!(text_of(task, "/fields/status/statusCategory/key") == "done");
    found
}

fn attachment(file: &Value) -> Value {
    json!({
        "id": text_of(file, "/id"),
        "name": text_of(file, "/filename"),
        "url": text_of(file, "/content"),
        "mime": file["mimeType"],
        "size": file["size"],
    })
}

pub async fn issue(api: &Api<'_>, issue: &str) -> Result<Value, ActionError> {
    let mut fields: Vec<&str> = ISSUE_FIELDS.to_vec();
    fields.push(&api.flag_field);
    let answer = read(api, issue, &fields).await?;
    let fields = &answer["fields"];
    let kind = text_of(fields, "/issuetype/name");
    let mut found = summary(api, &answer);
    let extra = json!({
        "description": markdown(&fields["description"]),
        "type": if kind.is_empty() { Value::Null } else { json!(kind) },
        "labels": fields["labels"].as_array().cloned().unwrap_or_default(),
        "assignee": person(&fields["assignee"]),
        "impediment": { "on": flagged(fields.get(&api.flag_field)), "reason": null },
        "subtasks": listed(fields, "subtasks").iter().map(|task| subtask(api, task)).collect::<Vec<_>>(),
        "attachments": listed(fields, "attachment").iter().map(attachment).collect::<Vec<_>>(),
        "comments": comments(fields),
        "updated": text_of(fields, "/updated"),
    });
    if let (Some(target), Some(more)) = (found.as_object_mut(), extra.as_object()) {
        target.extend(more.clone());
    }
    Ok(found)
}

fn same_text(left: &str, right: &str) -> bool {
    let words = |text: &str| jql_text(text).to_lowercase();
    words(left) == words(right)
}

pub async fn comment(api: &Api<'_>, issue: &str, body: &str, key: &str) -> Result<Value, ActionError> {
    let issue = jql_key(issue)?;
    if !key.is_empty() {
        let answer = read(api, issue, &["comment"]).await?;
        if let Some(found) =
            comments(&answer["fields"]).into_iter().find(|known| same_text(&text_of(known, "/body"), body))
        {
            let mut found = found;
            found["created"] = json!(false);
            return Ok(found);
        }
    }
    let document = markdown_to_adf(body).to_string();
    let arguments = json!({ "issueIdOrKey": issue, "commentBody": document, "contentFormat": "adf" });
    let answer = api.tool("addCommentToJiraIssue", arguments).await?;
    let mut made = comment_of(&answer);
    made["body"] = json!(body);
    made["created"] = json!(true);
    Ok(made)
}

fn remember(seen: &mut Vec<Value>, status: &Value) {
    let id = text_of(status, "/id");
    if !id.is_empty() && !seen.iter().any(|known| known["id"] == id.as_str()) {
        let category = text_of(status, "/statusCategory/key");
        seen.push(json!({
            "id": id,
            "name": text_of(status, "/name"),
            "category": if category.is_empty() { Value::Null } else { json!(category) },
        }));
    }
}

pub async fn statuses(api: &Api<'_>, project: &str) -> Result<Value, ActionError> {
    let jql = format!("project = \"{project}\" ORDER BY updated DESC");
    let page = api.search(&jql, &["status"], None).await?;
    let issues = listed(&page, "issues");
    let mut seen: Vec<Value> = Vec::new();
    issues.iter().for_each(|issue| remember(&mut seen, &issue["fields"]["status"]));
    if let Some(first) = issues.first() {
        let arguments = json!({ "issueIdOrKey": text_of(first, "/key") });
        let page = api.tool("getTransitionsForJiraIssue", arguments).await?;
        listed(&page, "transitions").iter().for_each(|transition| remember(&mut seen, &transition["to"]));
    }
    Ok(Value::Array(seen))
}

pub async fn transition(api: &Api<'_>, issue: &str, target: &str) -> Result<Value, ActionError> {
    let issue = jql_key(issue)?;
    let current = read(api, issue, &["status"]).await?;
    let now = text_of(&current, "/fields/status/name");
    if now.eq_ignore_ascii_case(target) {
        return Ok(json!({ "ref": issue, "status": now, "changed": false }));
    }
    let page = api.tool("getTransitionsForJiraIssue", json!({ "issueIdOrKey": issue })).await?;
    let transitions = listed(&page, "transitions");
    let found = transitions
        .iter()
        .find(|transition| text_of(transition, "/to/name").eq_ignore_ascii_case(target))
        .ok_or_else(|| ActionError::permanent(format!("{issue} cannot go from {now} to {target}")))?;
    let arguments = json!({ "issueIdOrKey": issue, "transition": { "id": text_of(found, "/id") } });
    api.tool("transitionJiraIssue", arguments).await?;
    Ok(json!({ "ref": issue, "status": text_of(found, "/to/name"), "changed": true }))
}

pub async fn set_impediment(api: &Api<'_>, args: Args<'_>, key: &str) -> Result<Value, ActionError> {
    let issue = jql_key(args.text("issue")?)?;
    let on = args.flag("on")?;
    let value = if on { json!([{ "value": "Impediment" }]) } else { Value::Null };
    let mut fields = Map::new();
    fields.insert(api.flag_field.clone(), value);
    api.tool("editJiraIssue", json!({ "issueIdOrKey": issue, "fields": fields })).await?;
    let reason = args.optional_text("reason").filter(|_| on);
    if let Some(reason) = reason {
        comment(api, issue, reason, key).await?;
    }
    Ok(json!({ "on": on, "reason": reason }))
}

pub async fn search(api: &Api<'_>, args: Args<'_>) -> Result<Value, ActionError> {
    let mut clauses = Vec::new();
    if let Some(project) = args.optional_text("project").or(api.project.as_deref()) {
        clauses.push(format!("project = \"{}\"", jql_key(project)?));
    }
    if let Some(status) = args.optional_text("status") {
        clauses.push(format!("status = \"{}\"", jql_text(status)));
    }
    if let Some(text) = args.optional_text("query").map(jql_text).filter(|text| !text.is_empty()) {
        clauses.push(format!("text ~ \"{text}\""));
    }
    if clauses.is_empty() {
        return Err(ActionError::permanent("a search needs a project, a status or a query"));
    }
    let jql = format!("{} ORDER BY updated DESC", clauses.join(" AND "));
    let limit = args.optional_number("limit").unwrap_or(DEFAULT_LIMIT).clamp(1, 100);
    let page = api.search(&jql, &SUMMARY_FIELDS, args.optional_text("cursor")).await?;
    let issues: Vec<Value> = listed(&page, "issues")
        .iter()
        .take(usize::try_from(limit).unwrap_or(usize::MAX))
        .map(|issue| summary(api, issue))
        .collect();
    let cursor = page["nextPageToken"].as_str().filter(|_| page["isLast"] != json!(true));
    Ok(json!({ "issues": issues, "cursor": cursor }))
}

pub async fn projects(api: &Api<'_>, args: Args<'_>) -> Result<Value, ActionError> {
    Ok(Value::Array(visible_projects(&api.mcp, &api.cloud, args.optional_text("query")).await?))
}
