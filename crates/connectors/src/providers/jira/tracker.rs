use serde_json::{Map, Value, json};

use super::adf::{adf_to_markdown, markdown_to_adf};
use super::{Api, jql_key, jql_text};
use crate::error::ActionError;
use crate::http::Method;
use crate::idempotency;
use crate::provider::{Args, text_of};

const ISSUE_FIELDS: &str = "summary,description,status,labels,attachment,subtasks,comment,created,updated";
const DEFAULT_LIMIT: u64 = 50;

fn property(key: &str) -> Value {
    json!([{ "key": idempotency::property_key(), "value": { "key": key } }])
}

fn flagged(value: Option<&Value>) -> bool {
    value.and_then(Value::as_array).is_some_and(|flags| !flags.is_empty())
}

fn created(api: &Api<'_>, issue: &Value, fresh: bool) -> Value {
    let key = text_of(issue, "/key");
    json!({ "id": text_of(issue, "/id"), "url": api.browse(&key), "key": key, "created": fresh })
}

async fn find_issue(api: &Api<'_>, project: &str, title: &str, key: &str) -> Result<Option<Value>, ActionError> {
    let words = jql_text(title);
    let mut jql = format!("project = \"{}\"", jql_key(project)?);
    if !words.is_empty() {
        jql.push_str(&format!(" AND summary ~ \"{words}\""));
    }
    jql.push_str(" ORDER BY created DESC");
    let name = idempotency::property_key();
    let body = json!({ "jql": jql, "fields": ["summary"], "properties": [name], "maxResults": 50 });
    let page = api.send(Method::Post, "/search/jql", &body).await?;
    let issues = page["issues"].as_array().map(Vec::as_slice).unwrap_or_default();
    Ok(issues.iter().find(|issue| issue["properties"][&name]["key"].as_str() == Some(key)).cloned())
}

pub async fn create_issue(api: &Api<'_>, args: Args<'_>, key: &str) -> Result<Value, ActionError> {
    let project = args.text("project")?;
    let title = args.text("title")?;
    if !key.is_empty()
        && let Some(found) = find_issue(api, project, title, key).await?
    {
        return Ok(created(api, &found, false));
    }
    let parent = args.optional_text("parent");
    let kind = args.optional_text("issue_type").unwrap_or(if parent.is_some() { "Subtask" } else { "Task" });
    let mut fields = json!({
        "project": { "key": project },
        "summary": title,
        "issuetype": { "name": kind },
        "description": markdown_to_adf(args.optional_text("description").unwrap_or_default()),
    });
    if let Some(parent) = parent {
        fields["parent"] = json!({ "key": parent });
    }
    if !args.list("labels").is_empty() {
        fields["labels"] = json!(args.list("labels"));
    }
    let mut body = json!({ "fields": fields });
    if !key.is_empty() {
        body["properties"] = property(key);
    }
    let answer = api.send(Method::Post, "/issue", &body).await?;
    Ok(created(api, &answer, true))
}

fn comments(fields: &Value) -> Vec<Value> {
    let listed = fields["comment"]["comments"].as_array().map(Vec::as_slice).unwrap_or_default();
    listed
        .iter()
        .map(|comment| {
            json!({
                "id": text_of(comment, "/id"),
                "author": text_of(comment, "/author/displayName"),
                "body": adf_to_markdown(&comment["body"]),
                "at": text_of(comment, "/created"),
            })
        })
        .collect()
}

pub async fn issue(api: &Api<'_>, issue: &str) -> Result<Value, ActionError> {
    let path = format!("/issue/{}?fields={ISSUE_FIELDS},{}", jql_key(issue)?, api.flag_field);
    let answer = api.get(&path).await?;
    let fields = &answer["fields"];
    let subtasks: Vec<Value> = fields["subtasks"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .map(|task| {
            json!({
                "key": text_of(task, "/key"),
                "title": text_of(task, "/fields/summary"),
                "status": text_of(task, "/fields/status/name"),
            })
        })
        .collect();
    let attachments: Vec<Value> = fields["attachment"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .map(|file| json!({ "name": text_of(file, "/filename"), "size": file["size"], "url": text_of(file, "/content") }))
        .collect();
    let key = text_of(&answer, "/key");
    Ok(json!({
        "url": api.browse(&key),
        "key": key,
        "title": text_of(fields, "/summary"),
        "description": adf_to_markdown(&fields["description"]),
        "status": text_of(fields, "/status/name"),
        "impediment": flagged(fields.get(&api.flag_field)),
        "labels": fields["labels"],
        "subtasks": subtasks,
        "attachments": attachments,
        "comments": comments(fields),
        "updated": text_of(fields, "/updated"),
    }))
}

fn carries_key(comment: &Value, key: &str) -> bool {
    let name = idempotency::property_key();
    comment["properties"].as_array().is_some_and(|properties| {
        properties.iter().any(|property| property["key"] == name.as_str() && property["value"]["key"] == key)
    })
}

pub async fn comment(api: &Api<'_>, issue: &str, body: &str, key: &str) -> Result<Value, ActionError> {
    let issue = jql_key(issue)?;
    if !key.is_empty() {
        let page =
            api.get(&format!("/issue/{issue}/comment?expand=properties&orderBy=-created&maxResults=100")).await?;
        let listed = page["comments"].as_array().map(Vec::as_slice).unwrap_or_default();
        if let Some(found) = listed.iter().find(|comment| carries_key(comment, key)) {
            return Ok(json!({ "id": text_of(found, "/id"), "created": false }));
        }
    }
    let mut request = json!({ "body": markdown_to_adf(body) });
    if !key.is_empty() {
        request["properties"] = property(key);
    }
    let answer = api.send(Method::Post, &format!("/issue/{issue}/comment"), &request).await?;
    Ok(json!({ "id": text_of(&answer, "/id"), "created": true }))
}

pub async fn statuses(api: &Api<'_>, project: &str) -> Result<Value, ActionError> {
    let answer = api.get(&format!("/project/{}/statuses", jql_key(project)?)).await?;
    let mut seen: Vec<Value> = Vec::new();
    for kind in answer.as_array().map(Vec::as_slice).unwrap_or_default() {
        for status in kind["statuses"].as_array().map(Vec::as_slice).unwrap_or_default() {
            let id = text_of(status, "/id");
            if !seen.iter().any(|known| known["id"] == id.as_str()) {
                seen.push(json!({
                    "id": id,
                    "name": text_of(status, "/name"),
                    "category": text_of(status, "/statusCategory/key"),
                }));
            }
        }
    }
    Ok(json!({ "statuses": seen }))
}

pub async fn transition(api: &Api<'_>, issue: &str, target: &str) -> Result<Value, ActionError> {
    let issue = jql_key(issue)?;
    let current = api.get(&format!("/issue/{issue}?fields=status")).await?;
    let now = text_of(&current, "/fields/status/name");
    if now.eq_ignore_ascii_case(target) {
        return Ok(json!({ "status": now, "changed": false }));
    }
    let page = api.get(&format!("/issue/{issue}/transitions")).await?;
    let listed = page["transitions"].as_array().map(Vec::as_slice).unwrap_or_default();
    let found = listed
        .iter()
        .find(|transition| text_of(transition, "/to/name").eq_ignore_ascii_case(target))
        .ok_or_else(|| ActionError::permanent(format!("{issue} cannot go from {now} to {target}")))?;
    let body = json!({ "transition": { "id": text_of(found, "/id") } });
    api.send(Method::Post, &format!("/issue/{issue}/transitions"), &body).await?;
    Ok(json!({ "status": text_of(found, "/to/name"), "changed": true }))
}

pub async fn set_impediment(api: &Api<'_>, args: Args<'_>, key: &str) -> Result<Value, ActionError> {
    let issue = jql_key(args.text("issue")?)?;
    let on = args.flag("on")?;
    let value = if on { json!([{ "value": "Impediment" }]) } else { Value::Null };
    let mut fields = Map::new();
    fields.insert(api.flag_field.clone(), value);
    api.send(Method::Put, &format!("/issue/{issue}"), &json!({ "fields": fields })).await?;
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
    let body = json!({ "jql": jql, "fields": fields, "maxResults": limit });
    let page = api.send(Method::Post, "/search/jql", &body).await?;
    let issues: Vec<Value> = page["issues"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
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
