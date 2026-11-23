use serde_json::{Map, Value, json};

use super::adf::{adf_to_markdown, markdown_to_adf};
use super::{Api, jql_key, jql_text, listed};
use crate::error::ActionError;
use crate::idempotency;
use crate::provider::{Args, text_of};

const ISSUE_FIELDS: [&str; 9] =
    ["summary", "description", "status", "labels", "attachment", "subtasks", "comment", "created", "updated"];
const DEFAULT_LIMIT: u64 = 50;

pub fn markdown(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Null => String::new(),
        other => adf_to_markdown(other),
    }
}

fn flagged(value: Option<&Value>) -> bool {
    value.and_then(Value::as_array).is_some_and(|flags| !flags.is_empty())
}

fn created(api: &Api<'_>, issue: &Value, fresh: bool) -> Value {
    let key = text_of(issue, "/key");
    json!({ "id": text_of(issue, "/id"), "url": api.browse(&key), "key": key, "created": fresh })
}

async fn find_issue(api: &Api<'_>, project: &str, key: &str) -> Result<Option<Value>, ActionError> {
    let jql = format!(
        "project = \"{}\" AND labels = \"{}\" ORDER BY created DESC",
        jql_key(project)?,
        idempotency::label(key)
    );
    let page = api.search(&jql, &["summary"], None).await?;
    Ok(listed(&page, "issues").first().cloned())
}

pub async fn create_issue(api: &Api<'_>, args: Args<'_>, key: &str) -> Result<Value, ActionError> {
    let project = args.text("project")?;
    let title = args.text("title")?;
    if !key.is_empty()
        && let Some(found) = find_issue(api, project, key).await?
    {
        return Ok(created(api, &found, false));
    }
    let parent = args.optional_text("parent");
    let kind = args.optional_text("issue_type").unwrap_or(if parent.is_some() { "Subtask" } else { "Task" });
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
    Ok(created(api, &answer, true))
}

fn comments(fields: &Value) -> Vec<Value> {
    listed(&fields["comment"], "comments")
        .iter()
        .map(|comment| {
            json!({
                "id": text_of(comment, "/id"),
                "author": text_of(comment, "/author/displayName"),
                "body": markdown(&comment["body"]),
                "at": text_of(comment, "/created"),
            })
        })
        .collect()
}

async fn read(api: &Api<'_>, issue: &str, fields: &[&str]) -> Result<Value, ActionError> {
    let arguments = json!({ "issueIdOrKey": jql_key(issue)?, "fields": fields, "responseContentFormat": "markdown" });
    api.tool("getJiraIssue", arguments).await
}

pub async fn issue(api: &Api<'_>, issue: &str) -> Result<Value, ActionError> {
    let mut fields: Vec<&str> = ISSUE_FIELDS.to_vec();
    fields.push(&api.flag_field);
    let answer = read(api, issue, &fields).await?;
    let fields = &answer["fields"];
    let subtasks: Vec<Value> = listed(fields, "subtasks")
        .iter()
        .map(|task| {
            json!({
                "key": text_of(task, "/key"),
                "title": text_of(task, "/fields/summary"),
                "status": text_of(task, "/fields/status/name"),
            })
        })
        .collect();
    let attachments: Vec<Value> = listed(fields, "attachment")
        .iter()
        .map(|file| json!({ "name": text_of(file, "/filename"), "size": file["size"], "url": text_of(file, "/content") }))
        .collect();
    let key = text_of(&answer, "/key");
    Ok(json!({
        "url": api.browse(&key),
        "key": key,
        "title": text_of(fields, "/summary"),
        "description": markdown(&fields["description"]),
        "status": text_of(fields, "/status/name"),
        "impediment": flagged(fields.get(&api.flag_field)),
        "labels": fields["labels"],
        "subtasks": subtasks,
        "attachments": attachments,
        "comments": comments(fields),
        "updated": text_of(fields, "/updated"),
    }))
}

fn same_text(left: &str, right: &str) -> bool {
    let words = |text: &str| jql_text(text).to_lowercase();
    words(left) == words(right)
}

pub async fn comment(api: &Api<'_>, issue: &str, body: &str, key: &str) -> Result<Value, ActionError> {
    let issue = jql_key(issue)?;
    if !key.is_empty() {
        let answer = read(api, issue, &["comment"]).await?;
        if let Some(found) = comments(&answer["fields"]).iter().find(|known| same_text(&text_of(known, "/body"), body))
        {
            return Ok(json!({ "id": text_of(found, "/id"), "created": false }));
        }
    }
    let document = markdown_to_adf(body).to_string();
    let arguments = json!({ "issueIdOrKey": issue, "commentBody": document, "contentFormat": "adf" });
    let answer = api.tool("addCommentToJiraIssue", arguments).await?;
    Ok(json!({ "id": text_of(&answer, "/id"), "created": true }))
}

fn remember(seen: &mut Vec<Value>, status: &Value) {
    let id = text_of(status, "/id");
    if !id.is_empty() && !seen.iter().any(|known| known["id"] == id.as_str()) {
        seen.push(json!({
            "id": id,
            "name": text_of(status, "/name"),
            "category": text_of(status, "/statusCategory/key"),
        }));
    }
}

pub async fn statuses(api: &Api<'_>, project: &str) -> Result<Value, ActionError> {
    let jql = format!("project = \"{}\" ORDER BY updated DESC", jql_key(project)?);
    let page = api.search(&jql, &["status"], None).await?;
    let issues = listed(&page, "issues");
    let mut seen: Vec<Value> = Vec::new();
    issues.iter().for_each(|issue| remember(&mut seen, &issue["fields"]["status"]));
    if let Some(first) = issues.first() {
        let arguments = json!({ "issueIdOrKey": text_of(first, "/key") });
        let page = api.tool("getTransitionsForJiraIssue", arguments).await?;
        listed(&page, "transitions").iter().for_each(|transition| remember(&mut seen, &transition["to"]));
    }
    Ok(json!({ "statuses": seen }))
}

pub async fn transition(api: &Api<'_>, issue: &str, target: &str) -> Result<Value, ActionError> {
    let issue = jql_key(issue)?;
    let current = read(api, issue, &["status"]).await?;
    let now = text_of(&current, "/fields/status/name");
    if now.eq_ignore_ascii_case(target) {
        return Ok(json!({ "status": now, "changed": false }));
    }
    let page = api.tool("getTransitionsForJiraIssue", json!({ "issueIdOrKey": issue })).await?;
    let found = listed(&page, "transitions")
        .iter()
        .find(|transition| text_of(transition, "/to/name").eq_ignore_ascii_case(target))
        .ok_or_else(|| ActionError::permanent(format!("{issue} cannot go from {now} to {target}")))?;
    let arguments = json!({ "issueIdOrKey": issue, "transition": { "id": text_of(found, "/id") } });
    api.tool("transitionJiraIssue", arguments).await?;
    Ok(json!({ "status": text_of(found, "/to/name"), "changed": true }))
}

pub async fn set_impediment(api: &Api<'_>, args: Args<'_>, key: &str) -> Result<Value, ActionError> {
    let issue = jql_key(args.text("issue")?)?;
    let on = args.flag("on")?;
    let value = if on { json!([{ "value": "Impediment" }]) } else { Value::Null };
    let mut fields = Map::new();
    fields.insert(api.flag_field.clone(), value);
    api.tool("editJiraIssue", json!({ "issueIdOrKey": issue, "fields": fields })).await?;
    if on && let Some(reason) = args.optional_text("reason") {
        comment(api, issue, reason, key).await?;
    }
    Ok(json!({ "impediment": on }))
}

pub async fn search(api: &Api<'_>, args: Args<'_>) -> Result<Value, ActionError> {
    let mut clauses = Vec::new();
    if let Some(project) = args.optional_text("project") {
        clauses.push(format!("project = \"{}\"", jql_key(project)?));
    }
    if let Some(status) = args.optional_text("status") {
        clauses.push(format!("status = \"{}\"", jql_text(status)));
    }
    if let Some(text) = args.optional_text("text").map(jql_text).filter(|text| !text.is_empty()) {
        clauses.push(format!("text ~ \"{text}\""));
    }
    if clauses.is_empty() {
        return Err(ActionError::permanent("a search needs a project, a status or a text"));
    }
    let jql = format!("{} ORDER BY updated DESC", clauses.join(" AND "));
    let limit = args.optional_number("limit").unwrap_or(DEFAULT_LIMIT).clamp(1, 100);
    let fields = ["summary", "status", "updated", api.flag_field.as_str()];
    let page = api.search(&jql, &fields, None).await?;
    let issues: Vec<Value> = listed(&page, "issues")
        .iter()
        .take(usize::try_from(limit).unwrap_or(usize::MAX))
        .map(|issue| {
            json!({
                "key": text_of(issue, "/key"),
                "title": text_of(issue, "/fields/summary"),
                "status": text_of(issue, "/fields/status/name"),
                "impediment": flagged(issue["fields"].get(&api.flag_field)),
                "updated": text_of(issue, "/fields/updated"),
            })
        })
        .collect();
    Ok(json!({ "issues": issues }))
}
