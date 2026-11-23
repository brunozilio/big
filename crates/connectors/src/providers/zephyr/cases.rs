use serde_json::{Map, Value, json};

use super::{Api, NO_PROJECT, valid_key};
use crate::error::ActionError;
use crate::mcp;
use crate::provider::{Args, text_of};

const STATUSES: [(&str, &str); 4] =
    [("passed", "Pass"), ("failed", "Fail"), ("blocked", "Blocked"), ("not_run", "Not Executed")];

pub fn id_of(value: &Value, pointer: &str) -> String {
    match value.pointer(pointer) {
        Some(Value::String(text)) => text.clone(),
        Some(Value::Number(number)) => number.to_string(),
        _ => String::new(),
    }
}

fn same_title(left: &str, right: &str) -> bool {
    let words = |text: &str| text.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase();
    words(left) == words(right)
}

fn put_text(arguments: &mut Map<String, Value>, field: &str, value: Option<&str>) {
    if let Some(value) = value {
        arguments.insert(field.to_owned(), json!(value));
    }
}

fn issue_id(args: Args<'_>) -> Result<Option<u64>, ActionError> {
    match args.0.get("issue_id") {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(text)) if text.is_empty() => Ok(None),
        Some(_) => args
            .optional_number("issue_id")
            .map(Some)
            .ok_or_else(|| ActionError::permanent("`issue_id` is the numeric id of the issue")),
    }
}

async fn linked(api: &Api<'_>, issue: &str) -> Result<Vec<Value>, ActionError> {
    let answer = api.tool("zephyr_get_issue_link_test_cases", json!({ "issueKey": valid_key(issue)? })).await?;
    let mut found = Vec::new();
    for link in mcp::items(&answer, "values") {
        let key = text_of(&link, "/key");
        if key.is_empty() {
            continue;
        }
        let case = api.tool("zephyr_get_test_case", json!({ "testCaseKey": key })).await?;
        found.push(json!({ "ref": key, "title": text_of(&case, "/name") }));
    }
    Ok(found)
}

pub async fn cases(api: &Api<'_>, issue: &str) -> Result<Value, ActionError> {
    Ok(Value::Array(linked(api, issue).await?))
}

fn step_of(step: &Value) -> Value {
    let mut inline = Map::new();
    match step {
        Value::String(text) => {
            inline.insert("description".to_owned(), json!(text));
            inline.insert("expectedResult".to_owned(), json!(""));
        }
        other => {
            inline.insert("description".to_owned(), json!(text_of(other, "/action")));
            put_text(&mut inline, "testData", other["data"].as_str().filter(|data| !data.is_empty()));
            inline.insert("expectedResult".to_owned(), json!(text_of(other, "/expected")));
        }
    }
    json!({ "inline": inline })
}

fn new_case(project: &str, title: &str, args: Args<'_>) -> Value {
    let mut arguments = Map::new();
    arguments.insert("projectKey".to_owned(), json!(project));
    arguments.insert("name".to_owned(), json!(title));
    put_text(&mut arguments, "objective", args.optional_text("objective"));
    put_text(&mut arguments, "precondition", args.optional_text("precondition"));
    let labels: Vec<&str> = args.list("labels").iter().filter_map(Value::as_str).collect();
    if !labels.is_empty() {
        arguments.insert("labels".to_owned(), json!(labels));
    }
    Value::Object(arguments)
}

pub async fn create_case(api: &Api<'_>, args: Args<'_>) -> Result<Value, ActionError> {
    let title = args.text("title")?;
    let link = issue_id(args)?;
    if let Some(issue) = args.optional_text("issue")
        && let Some(found) =
            linked(api, issue).await?.into_iter().find(|case| same_title(&text_of(case, "/title"), title))
    {
        return Ok(json!({ "ref": found["ref"], "title": found["title"], "url": "", "created": false }));
    }
    let project = api.project(args)?;
    let made = api.tool("zephyr_create_test_case", new_case(project, title, args)).await?;
    let key = text_of(&made, "/key");
    if key.is_empty() {
        return Err(ActionError::decode("the new test case has no key"));
    }
    let steps: Vec<Value> = args.list("steps").iter().map(step_of).collect();
    if !steps.is_empty() {
        let arguments = json!({ "testCaseKey": key, "mode": "OVERWRITE", "items": steps });
        api.tool("zephyr_create_test_case_steps", arguments).await?;
    }
    if let Some(issue) = link {
        api.tool("zephyr_create_test_case_issue_link", json!({ "testCaseKey": key, "issueId": issue })).await?;
    }
    Ok(json!({ "ref": key, "title": title, "url": "", "created": true }))
}

pub async fn create_cycle(api: &Api<'_>, args: Args<'_>) -> Result<Value, ActionError> {
    let title = args.text("title")?;
    let link = issue_id(args)?;
    let project = api.project(args)?;
    let mut arguments = Map::new();
    arguments.insert("projectKey".to_owned(), json!(project));
    arguments.insert("name".to_owned(), json!(title));
    put_text(&mut arguments, "description", args.optional_text("description"));
    let made = api.tool("zephyr_create_test_cycle", Value::Object(arguments)).await?;
    let key = text_of(&made, "/key");
    if key.is_empty() {
        return Err(ActionError::decode("the new test cycle has no key"));
    }
    if let Some(issue) = link {
        let arguments = json!({ "testCycleIdOrKey": key, "issueId": issue });
        api.tool("zephyr_create_test_cycle_issue_link", arguments).await?;
    }
    Ok(json!({ "ref": key, "url": "" }))
}

fn status_name(status: &str) -> Result<&'static str, ActionError> {
    STATUSES.iter().find(|(name, _)| name.eq_ignore_ascii_case(status.trim())).map(|(_, remote)| *remote).ok_or_else(
        || {
            let known: Vec<&str> = STATUSES.iter().map(|(name, _)| *name).collect();
            ActionError::permanent(format!("{status:?} is not a test status; use one of {}", known.join(", ")))
        },
    )
}

fn case_project(case: &str) -> Option<&str> {
    case.rsplit_once("-T").map(|(project, _)| project).filter(|project| !project.is_empty())
}

pub async fn record(api: &Api<'_>, args: Args<'_>) -> Result<Value, ActionError> {
    let case = valid_key(args.text("case")?)?;
    let cycle = valid_key(args.text("cycle")?)?;
    let status = status_name(args.text("status")?)?;
    let project = args.optional_text("project").or(api.project.as_deref()).or_else(|| case_project(case));
    let project = valid_key(project.ok_or_else(|| ActionError::blocked(NO_PROJECT))?)?;
    let mut arguments = Map::new();
    arguments.insert("projectKey".to_owned(), json!(project));
    arguments.insert("testCaseKey".to_owned(), json!(case));
    arguments.insert("testCycleKey".to_owned(), json!(cycle));
    arguments.insert("statusName".to_owned(), json!(status));
    put_text(&mut arguments, "comment", args.optional_text("comment"));
    if let Some(seconds) = args.0.get("seconds").and_then(Value::as_f64).filter(|seconds| *seconds >= 0.0) {
        arguments.insert("executionTime".to_owned(), json!((seconds * 1000.0).round() as u64));
    }
    let made = api.tool("zephyr_create_test_execution", Value::Object(arguments)).await?;
    let id = id_of(&made, "/id");
    let key = text_of(&made, "/key");
    Ok(json!({ "ref": if key.is_empty() { id } else { key } }))
}
