use serde_json::{Map, Value, json};

use super::adf::{adf_to_markdown, markdown_to_adf};
use super::{Api, jql_key, jql_text};
use crate::error::ActionError;
use crate::http::Method;
use crate::idempotency;
use crate::provider::{Args, text_of};

const ISSUE_FIELDS: &str =
    "summary,description,status,labels,attachment,subtasks,comment,created,updated,issuetype,assignee";
const DEFAULT_LIMIT: u64 = 50;

fn property(key: &str) -> Value {
    json!([{ "key": idempotency::property_key(), "value": { "key": key } }])
}

fn flagged(value: Option<&Value>) -> bool {
    value.and_then(Value::as_array).is_some_and(|flags| !flags.is_empty())
}

fn created(api: &Api<'_>, issue: &Value, title: &str, fresh: bool) -> Value {
    let key = text_of(issue, "/key");
    json!({
        "ref": key,
        "id": text_of(issue, "/id"),
        "url": api.browse(&key),
        "title": title,
        "status": text_of(issue, "/fields/status/name"),
        "created": fresh,
    })
}

fn project_of<'a>(api: &'a Api<'_>, args: Args<'a>) -> Result<&'a str, ActionError> {
    args.optional_text("project").or(api.project.as_deref()).ok_or_else(|| ActionError::missing("project"))
}

async fn find_issue(api: &Api<'_>, project: &str, title: &str, key: &str) -> Result<Option<Value>, ActionError> {
    let words = jql_text(title);
    let mut jql = format!("project = \"{}\"", jql_key(project)?);
    if !words.is_empty() {
        jql.push_str(&format!(" AND summary ~ \"{words}\""));
    }
    jql.push_str(" ORDER BY created DESC");
    let name = idempotency::property_key();
    let body = json!({ "jql": jql, "fields": ["summary", "status"], "properties": [name], "maxResults": 50 });
    let page = api.send(Method::Post, "/search/jql", &body).await?;
    let issues = page["issues"].as_array().map(Vec::as_slice).unwrap_or_default();
    Ok(issues.iter().find(|issue| issue["properties"][&name]["key"].as_str() == Some(key)).cloned())
}

pub async fn create_issue(api: &Api<'_>, args: Args<'_>, key: &str) -> Result<Value, ActionError> {
    let project = project_of(api, args)?;
    let title = args.text("title")?;
    if !key.is_empty()
        && let Some(found) = find_issue(api, project, title, key).await?
    {
        return Ok(created(api, &found, title, false));
    }
    let parent = args.optional_text("parent");
    let named = args.optional_text("type").or_else(|| args.optional_text("issue_type"));
    let kind = named.unwrap_or(if parent.is_some() { "Subtask" } else { "Task" });
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
    Ok(created(api, &answer, title, true))
}

fn comments(fields: &Value) -> Vec<Value> {
    let listed = fields["comment"]["comments"].as_array().map(Vec::as_slice).unwrap_or_default();
    listed
        .iter()
        .map(|comment| {
            json!({
                "id": text_of(comment, "/id"),
                "author": person(&comment["author"]),
                "body": adf_to_markdown(&comment["body"]),
                "at": text_of(comment, "/created"),
            })
        })
        .collect()
}

fn person(found: &Value) -> Value {
    if found.is_null() {
        return Value::Null;
    }
    json!({ "id": text_of(found, "/accountId"), "name": text_of(found, "/displayName") })
}

fn subtask(api: &Api<'_>, task: &Value) -> Value {
    let key = text_of(task, "/key");
    json!({
        "url": api.browse(&key),
        "ref": key,
        "title": text_of(task, "/fields/summary"),
        "status": text_of(task, "/fields/status/name"),
        "done": text_of(task, "/fields/status/statusCategory/key") == "done",
    })
}

fn attachment(file: &Value) -> Value {
    json!({
        "id": text_of(file, "/id"),
        "name": text_of(file, "/filename"),
        "url": text_of(file, "/content"),
        "mime": text_of(file, "/mimeType"),
        "size": file["size"],
    })
}

fn list(value: &Value) -> &[Value] {
    value.as_array().map(Vec::as_slice).unwrap_or_default()
}

pub async fn issue(api: &Api<'_>, issue: &str) -> Result<Value, ActionError> {
    let path = format!("/issue/{}?fields={ISSUE_FIELDS},{}", jql_key(issue)?, api.flag_field);
    let answer = api.get(&path).await?;
    let fields = &answer["fields"];
    let key = text_of(&answer, "/key");
    let kind = text_of(fields, "/issuetype/name");
    Ok(json!({
        "url": api.browse(&key),
        "ref": key,
        "title": text_of(fields, "/summary"),
        "description": adf_to_markdown(&fields["description"]),
        "status": text_of(fields, "/status/name"),
        "type": if kind.is_empty() { Value::Null } else { json!(kind) },
        "assignee": person(&fields["assignee"]),
        "impediment": { "on": flagged(fields.get(&api.flag_field)), "reason": null },
        "labels": fields["labels"].as_array().cloned().unwrap_or_default(),
        "subtasks": list(&fields["subtasks"]).iter().map(|task| subtask(api, task)).collect::<Vec<_>>(),
        "attachments": list(&fields["attachment"]).iter().map(attachment).collect::<Vec<_>>(),
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
            return Ok(comment_view(found, body, false));
        }
    }
    let mut request = json!({ "body": markdown_to_adf(body) });
    if !key.is_empty() {
        request["properties"] = property(key);
    }
    let answer = api.send(Method::Post, &format!("/issue/{issue}/comment"), &request).await?;
    Ok(comment_view(&answer, body, true))
}

fn comment_view(found: &Value, body: &str, fresh: bool) -> Value {
    json!({
        "id": text_of(found, "/id"),
        "author": person(&found["author"]),
        "body": body,
        "at": text_of(found, "/created"),
        "created": fresh,
    })
}

pub async fn statuses(api: &Api<'_>, args: Args<'_>) -> Result<Value, ActionError> {
    let project = project_of(api, args)?;
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
    Ok(Value::Array(seen))
}

pub async fn transition(api: &Api<'_>, issue: &str, target: &str) -> Result<Value, ActionError> {
    let issue = jql_key(issue)?;
    let current = api.get(&format!("/issue/{issue}?fields=status")).await?;
    let now = text_of(&current, "/fields/status/name");
    if now.eq_ignore_ascii_case(target) {
        return Ok(json!({ "ref": issue, "status": now, "changed": false }));
    }
    let page = api.get(&format!("/issue/{issue}/transitions")).await?;
    let listed = page["transitions"].as_array().map(Vec::as_slice).unwrap_or_default();
    let found = listed
        .iter()
        .find(|transition| text_of(transition, "/to/name").eq_ignore_ascii_case(target))
        .ok_or_else(|| ActionError::permanent(format!("{issue} cannot go from {now} to {target}")))?;
    let body = json!({ "transition": { "id": text_of(found, "/id") } });
    api.send(Method::Post, &format!("/issue/{issue}/transitions"), &body).await?;
    Ok(json!({ "ref": issue, "status": text_of(found, "/to/name"), "changed": true }))
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
    Ok(json!({ "on": on, "reason": if on { args.optional_text("reason") } else { None } }))
}

pub async fn search(api: &Api<'_>, args: Args<'_>) -> Result<Value, ActionError> {
    let mut clauses = Vec::new();
    if let Some(project) = args.optional_text("project") {
        clauses.push(format!("project = \"{}\"", jql_key(project)?));
    }
    if let Some(status) = args.optional_text("status") {
        clauses.push(format!("status = \"{}\"", jql_text(status)));
    }
    let text = args.optional_text("query").or_else(|| args.optional_text("text"));
    if let Some(text) = text.map(jql_text).filter(|text| !text.is_empty()) {
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
            let key = text_of(issue, "/key");
            json!({
                "url": api.browse(&key),
                "ref": key,
                "title": text_of(issue, "/fields/summary"),
                "status": text_of(issue, "/fields/status/name"),
                "impediment": flagged(issue["fields"].get(&api.flag_field)),
                "updated": text_of(issue, "/fields/updated"),
            })
        })
        .collect();
    Ok(json!({ "issues": issues, "cursor": null }))
}
