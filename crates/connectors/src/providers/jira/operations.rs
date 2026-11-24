use serde_json::{Value, json};

use crate::error::{ActionError, ErrorClass};
use crate::mcp::{self, McpClient};

const NAME_KEYS: [&str; 4] = ["name", "operation", "operationName", "operationId"];
const READ_VERBS: [&str; 6] = ["get", "list", "search", "find", "lookup", "fetch"];
const LIST_VERBS: [&str; 2] = ["list", "search"];
const SCOPES: [&str; 3] = ["result", "data", "response"];

#[derive(Debug, Clone, Copy)]
pub struct Operation {
    pub name: &'static str,
    pub topic: &'static str,
    pub query: &'static str,
}

pub const PROJECTS: Operation =
    Operation { name: "listJiraProjects", topic: "project", query: "list jira projects visible to the user" };
pub const TRANSITIONS: Operation =
    Operation { name: "listJiraIssueTransitions", topic: "transition", query: "list transitions of a jira issue" };

pub const STATUSES: Operation =
    Operation { name: "listJiraStatuses", topic: "status", query: "list the statuses of a jira project" };
pub const ISSUE_TYPES: Operation = Operation {
    name: "listJiraProjectIssueTypesMetadata",
    topic: "type",
    query: "list the issue types of a jira project",
};
pub const TYPE_FIELDS: Operation =
    Operation { name: "getJiraIssueTypeMetaWithFields", topic: "field", query: "get the fields of a jira issue type" };

async fn execute_read(mcp: &McpClient<'_>, cloud: &str, name: &str, inputs: &Value) -> Result<Value, ActionError> {
    mcp.call("executeRead", json!({ "cloudId": cloud, "name": name, "inputs": inputs })).await
}

pub async fn read(
    mcp: &McpClient<'_>,
    cloud: &str,
    operation: &Operation,
    inputs: &Value,
) -> Result<(String, Value), ActionError> {
    let refused = match execute_read(mcp, cloud, operation.name, inputs).await {
        Ok(found) => return Ok((operation.name.to_owned(), found)),
        Err(error) if error.class != ErrorClass::Permanent => return Err(error),
        Err(error) => error,
    };
    let found = mcp.call("discover", json!({ "query": operation.query })).await?;
    let Some(name) = discovered(&found, operation.topic, operation.name) else { return Err(refused) };
    let answer = execute_read(mcp, cloud, &name, inputs).await?;
    Ok((name, answer))
}

pub async fn read_named(mcp: &McpClient<'_>, cloud: &str, name: &str, inputs: &Value) -> Result<Value, ActionError> {
    execute_read(mcp, cloud, name, inputs).await
}

fn operation_name(word: &str) -> bool {
    let lower = word.to_ascii_lowercase();
    word.len() >= 6
        && word.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        && word.chars().next().is_some_and(|c| c.is_ascii_lowercase())
        && word.chars().any(|c| c.is_ascii_uppercase())
        && READ_VERBS.iter().any(|verb| lower.starts_with(verb))
}

fn collect(value: &Value, found: &mut Vec<(String, String)>) {
    match value {
        Value::Object(fields) => {
            let named = NAME_KEYS
                .iter()
                .filter_map(|key| fields.get(*key).and_then(Value::as_str))
                .find(|name| operation_name(name));
            if let Some(name) = named {
                found.push((name.to_owned(), value.to_string().to_lowercase()));
            }
            fields.values().for_each(|inner| collect(inner, found));
        }
        Value::Array(items) => items.iter().for_each(|inner| collect(inner, found)),
        Value::String(text) => text.lines().for_each(|line| {
            let words =
                line.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_')).filter(|word| operation_name(word));
            found.extend(words.map(|word| (word.to_owned(), line.to_lowercase())));
        }),
        _ => {}
    }
}

pub fn discovered(value: &Value, topic: &str, tried: &str) -> Option<String> {
    let mut found = Vec::new();
    collect(&mcp::payload(value.clone()), &mut found);
    found.retain(|(name, text)| name != tried && text.contains(topic));
    let score = |name: &str| {
        let lower = name.to_ascii_lowercase();
        (lower.contains(topic), LIST_VERBS.iter().any(|verb| lower.starts_with(verb)))
    };
    let best = found.iter().map(|(name, _)| score(name)).max()?;
    found.into_iter().map(|(name, _)| name).find(|name| score(name) == best)
}

pub fn found_list(value: &Value, fields: &[&str]) -> Vec<Value> {
    let value = mcp::payload(value.clone());
    let scopes = std::iter::once(&value).chain(SCOPES.iter().filter_map(|scope| value.get(*scope)));
    for scope in scopes {
        if let Some(found) = fields.iter().map(|field| mcp::items(scope, field)).find(|found| !found.is_empty()) {
            return found;
        }
    }
    Vec::new()
}

pub fn last_page(value: &Value) -> bool {
    std::iter::once(value)
        .chain(SCOPES.iter().filter_map(|scope| value.get(*scope)))
        .any(|scope| scope["isLast"] == json!(true))
}

pub fn id_of(value: &Value, pointer: &str) -> String {
    match value.pointer(pointer) {
        Some(Value::String(text)) => text.clone(),
        Some(Value::Number(number)) => number.to_string(),
        _ => String::new(),
    }
}
