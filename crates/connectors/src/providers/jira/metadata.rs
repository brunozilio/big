use serde_json::{Map, Value, json};

use super::operations::{self, found_list, id_of};
use super::{Api, listed};
use crate::error::{ActionError, ErrorClass};
use crate::provider::{Args, text_of};

const FILLED: [&str; 6] = ["project", "issuetype", "summary", "description", "parent", "labels"];
const SAMPLE_FIELDS: [&str; 2] = ["status", "issuetype"];

pub fn kind_of<'a>(args: Args<'a>) -> &'a str {
    let default = if args.optional_text("parent").is_some() { "Subtask" } else { "Task" };
    args.optional_text("type").unwrap_or(default)
}

fn category(status: &Value) -> Value {
    let key = text_of(status, "/statusCategory/key");
    let key = if key.is_empty() { text_of(status, "/statusCategory") } else { key };
    let named = match key.to_ascii_uppercase().as_str() {
        "" => return Value::Null,
        "TODO" | "NEW" => "new",
        "IN_PROGRESS" | "INDETERMINATE" => "indeterminate",
        "DONE" => "done",
        _ => return json!(key),
    };
    json!(named)
}

fn remember(seen: &mut Vec<Value>, status: &Value) {
    let id = id_of(status, "/id");
    let name = text_of(status, "/name");
    let known = |found: &Value| if id.is_empty() { found["name"] == name.as_str() } else { found["id"] == id.as_str() };
    if !name.is_empty() && !seen.iter().any(known) {
        seen.push(
            json!({ "id": if id.is_empty() { name.clone() } else { id }, "name": name, "category": category(status) }),
        );
    }
}

fn listed_statuses(answer: &Value) -> Vec<Value> {
    let mut seen = Vec::new();
    for entry in found_list(answer, &["values", "statuses", "issueTypes"]) {
        let inner = listed(&entry, "statuses");
        if inner.is_empty() {
            remember(&mut seen, &entry);
        } else {
            inner.iter().for_each(|status| remember(&mut seen, status));
        }
    }
    seen
}

async fn sampled(api: &Api<'_>, project: &str) -> Result<Vec<Value>, ActionError> {
    let jql = format!("project = \"{project}\" ORDER BY updated DESC");
    let page = api.search(&jql, &SAMPLE_FIELDS, None).await?;
    let issues = listed(&page, "issues");
    let mut seen: Vec<Value> = Vec::new();
    issues.iter().for_each(|issue| remember(&mut seen, &issue["fields"]["status"]));
    let mut kinds: Vec<String> = Vec::new();
    for issue in &issues {
        let kind = text_of(issue, "/fields/issuetype/name");
        if kinds.contains(&kind) {
            continue;
        }
        kinds.push(kind);
        let transitions = api.transitions(&text_of(issue, "/key")).await?;
        transitions.iter().for_each(|transition| remember(&mut seen, &transition["to"]));
    }
    Ok(seen)
}

pub async fn statuses(api: &Api<'_>, project: &str) -> Result<Value, ActionError> {
    let inputs = json!({ "projectIdOrKey": project });
    let found = match operations::read(&api.mcp, &api.cloud, &operations::STATUSES, &inputs).await {
        Ok((_, answer)) => listed_statuses(&answer),
        Err(error) if error.class == ErrorClass::Permanent => Vec::new(),
        Err(error) => return Err(error),
    };
    if !found.is_empty() {
        return Ok(Value::Array(found));
    }
    Ok(Value::Array(sampled(api, project).await?))
}

async fn issue_type(api: &Api<'_>, project: &str, kind: &str) -> Result<String, ActionError> {
    let inputs = json!({ "projectIdOrKey": project, "maxResults": 200 });
    let (_, answer) = operations::read(&api.mcp, &api.cloud, &operations::ISSUE_TYPES, &inputs).await?;
    let named =
        |found: &&Value| ["/name", "/untranslatedName"].iter().any(|at| text_of(found, at).eq_ignore_ascii_case(kind));
    found_list(&answer, &["issueTypes", "values"])
        .iter()
        .find(named)
        .map(|found| id_of(found, "/id"))
        .ok_or_else(|| ActionError::permanent(format!("{project} has no issue type {kind}")))
}

fn field_key(field: &Value) -> String {
    let key = text_of(field, "/key");
    if key.is_empty() { text_of(field, "/fieldId") } else { key }
}

async fn required(api: &Api<'_>, project: &str, kind: &str) -> Result<Vec<Value>, ActionError> {
    let id = issue_type(api, project, kind).await?;
    let inputs = json!({ "projectIdOrKey": project, "issueTypeId": id, "requiredFieldsOnly": true, "maxResults": 200 });
    let (_, answer) = operations::read(&api.mcp, &api.cloud, &operations::TYPE_FIELDS, &inputs).await?;
    Ok(found_list(&answer, &["fields", "values"])
        .into_iter()
        .filter(|field| field["required"] != json!(false) && field["hasDefaultValue"] != json!(true))
        .filter(|field| !FILLED.contains(&field_key(field).as_str()))
        .collect())
}

fn choice(value: &Value) -> Value {
    let name = ["/name", "/value", "/key"].iter().map(|at| text_of(value, at)).find(|name| !name.is_empty());
    json!({ "id": id_of(value, "/id"), "name": name.unwrap_or_default() })
}

fn field_of(field: &Value) -> Value {
    let values: Vec<Value> = listed(field, "allowedValues").iter().map(choice).collect();
    let mut found = json!({
        "key": field_key(field), "name": text_of(field, "/name"), "type": text_of(field, "/schema/type"), "required": true,
    });
    if !values.is_empty() {
        found["values"] = json!(values);
    }
    found
}

pub async fn fields(api: &Api<'_>, args: Args<'_>) -> Result<Value, ActionError> {
    let project = api.project(args)?;
    let found = required(api, project, kind_of(args)).await?;
    Ok(Value::Array(found.iter().map(field_of).collect()))
}

fn one(kind: &str, choices: bool, text: &str) -> Value {
    if choices {
        return json!({ "id": text });
    }
    match kind {
        "user" => json!({ "accountId": text }),
        "number" => text.trim().parse::<f64>().map_or_else(|_| json!(text), |number| json!(number)),
        _ => json!(text),
    }
}

fn shaped(field: Option<&Value>, value: &Value) -> Value {
    let (Some(field), Some(text)) = (field, value.as_str()) else { return value.clone() };
    let choices = !listed(field, "allowedValues").is_empty();
    let kind = text_of(field, "/schema/type");
    if kind == "array" {
        return json!([one(&text_of(field, "/schema/items"), choices, text)]);
    }
    one(&kind, choices, text)
}

pub async fn extra(api: &Api<'_>, project: &str, args: Args<'_>) -> Result<Option<Value>, ActionError> {
    let Some(given) = args.0["fields"].as_object().filter(|given| !given.is_empty()) else { return Ok(None) };
    let known = required(api, project, kind_of(args)).await?;
    let sent: Map<String, Value> = given
        .iter()
        .map(|(key, value)| (key.clone(), shaped(known.iter().find(|field| field_key(field) == *key), value)))
        .collect();
    Ok(Some(Value::Object(sent)))
}
