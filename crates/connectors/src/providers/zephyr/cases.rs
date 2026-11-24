use serde_json::{Map, Value, json};

use super::{Api, MOST_PAGES, NO_PROJECT, PAGE, last_page, valid_key};
use crate::error::ActionError;
use crate::idempotency;
use crate::mcp;
use crate::provider::{Args, text_of};

const LOOKUP_PAGE: u64 = 1000;

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

const CASE_FOLDERS: &str = "TEST_CASE";

async fn existing_folder(api: &Api<'_>, project: &str, name: &str) -> Result<Option<Value>, ActionError> {
    for page in 0..MOST_PAGES {
        let wanted =
            json!({ "projectKey": project, "folderType": CASE_FOLDERS, "startAt": page * PAGE, "maxResults": PAGE });
        let answer = api.tool("zephyr_get_folders", wanted).await?;
        let folders = mcp::items(&answer, "values");
        let found =
            folders.iter().find(|folder| folder["parentId"].is_null() && same_title(&text_of(folder, "/name"), name));
        if let Some(found) = found {
            return Ok(Some(found.clone()));
        }
        if last_page(&answer, folders.len()) {
            break;
        }
    }
    Ok(None)
}

async fn folder_id(api: &Api<'_>, project: &str, name: &str) -> Result<Value, ActionError> {
    let folder = match existing_folder(api, project, name).await? {
        Some(found) => found,
        None => {
            let wanted = json!({ "projectKey": project, "name": name, "folderType": CASE_FOLDERS });
            api.tool("zephyr_create_folder", wanted).await?
        }
    };
    match &folder["id"] {
        Value::Number(id) => Ok(Value::Number(id.clone())),
        Value::String(id) => Ok(id.parse::<u64>().map_or_else(|_| json!(id), |id| json!(id))),
        _ => Err(ActionError::decode("the test folder has no id")),
    }
}

fn with_marker(text: Option<&str>, key: &str) -> Option<String> {
    match text {
        _ if !key.is_empty() => Some(idempotency::tagged(text.unwrap_or_default(), key)),
        Some(text) => Some(text.to_owned()),
        None => None,
    }
}

fn reference(found: &Value) -> String {
    let key = text_of(found, "/key");
    if key.is_empty() { id_of(found, "/id") } else { key }
}

async fn by_cursor(
    api: &Api<'_>,
    tool: &str,
    mut wanted: Value,
    matches: impl Fn(&Value) -> bool,
) -> Result<Option<Value>, ActionError> {
    for _ in 0..MOST_PAGES {
        let answer = api.tool(tool, wanted.clone()).await?;
        let values = mcp::items(&answer, "values");
        if let Some(found) = values.iter().find(|value| matches(value)) {
            return Ok(Some(found.clone()));
        }
        match answer["nextStartAtId"].as_u64() {
            Some(next) if !values.is_empty() => wanted["startAtId"] = json!(next),
            _ => break,
        }
    }
    Ok(None)
}

async fn keyed_case(api: &Api<'_>, project: &str, title: &str, key: &str) -> Result<Option<Value>, ActionError> {
    if key.is_empty() {
        return Ok(None);
    }
    let label = json!(idempotency::label(key));
    let wanted = json!({ "projectKey": project, "nameOrKey": title, "limit": PAGE });
    let labelled = |case: &Value| case["labels"].as_array().is_some_and(|labels| labels.contains(&label));
    by_cursor(api, "zephyr_get_test_cases", wanted, labelled).await
}

async fn keyed_cycle(api: &Api<'_>, project: &str, key: &str) -> Result<Option<Value>, ActionError> {
    if key.is_empty() {
        return Ok(None);
    }
    let mut start = 0;
    for _ in 0..MOST_PAGES {
        let wanted = json!({ "projectKey": project, "maxResults": LOOKUP_PAGE, "startAt": start });
        let answer = api.tool("zephyr_get_test_cycles", wanted).await?;
        let cycles = mcp::items(&answer, "values");
        if let Some(found) = cycles.iter().find(|cycle| idempotency::carries_tag(&text_of(cycle, "/description"), key)) {
            return Ok(Some(found.clone()));
        }
        let fetched = u64::try_from(cycles.len()).unwrap_or(0);
        let done = match answer["isLast"].as_bool() {
            Some(last) => last || fetched == 0,
            None => fetched < LOOKUP_PAGE,
        };
        if done {
            break;
        }
        start += fetched;
    }
    Ok(None)
}

fn new_case(project: &str, title: &str, args: Args<'_>, key: &str) -> Value {
    let mut arguments = Map::new();
    arguments.insert("projectKey".to_owned(), json!(project));
    arguments.insert("name".to_owned(), json!(title));
    put_text(&mut arguments, "objective", args.optional_text("objective"));
    put_text(&mut arguments, "precondition", args.optional_text("precondition"));
    let mut labels: Vec<String> = args.list("labels").iter().filter_map(Value::as_str).map(str::to_owned).collect();
    if !key.is_empty() {
        labels.push(idempotency::label(key));
    }
    if !labels.is_empty() {
        arguments.insert("labels".to_owned(), json!(labels));
    }
    Value::Object(arguments)
}

pub async fn create_case(api: &Api<'_>, args: Args<'_>, key: &str) -> Result<Value, ActionError> {
    let title = args.text("title")?;
    let link = issue_id(args)?;
    if let Some(issue) = args.optional_text("issue")
        && let Some(found) =
            linked(api, issue).await?.into_iter().find(|case| same_title(&text_of(case, "/title"), title))
    {
        return Ok(json!({ "ref": found["ref"], "title": found["title"], "url": "", "created": false }));
    }
    let project = api.project(args)?;
    if let Some(found) = keyed_case(api, project, title, key).await? {
        return Ok(json!({ "ref": reference(&found), "title": text_of(&found, "/name"), "url": "", "created": false }));
    }
    let mut wanted = new_case(project, title, args, key);
    if let Some(folder) = args.optional_text("folder").map(str::trim).filter(|folder| !folder.is_empty()) {
        wanted["folderId"] = folder_id(api, project, folder).await?;
    }
    let made = api.tool("zephyr_create_test_case", wanted).await?;
    let case = text_of(&made, "/key");
    if case.is_empty() {
        return Err(ActionError::decode("the new test case has no key"));
    }
    let steps: Vec<Value> = args.list("steps").iter().map(step_of).collect();
    if !steps.is_empty() {
        let arguments = json!({ "testCaseKey": case, "mode": "OVERWRITE", "items": steps });
        api.tool("zephyr_create_test_case_steps", arguments).await?;
    }
    if let Some(issue) = link {
        api.tool("zephyr_create_test_case_issue_link", json!({ "testCaseKey": case, "issueId": issue })).await?;
    }
    Ok(json!({ "ref": case, "title": title, "url": "", "created": true }))
}

pub async fn create_cycle(api: &Api<'_>, args: Args<'_>, key: &str) -> Result<Value, ActionError> {
    let title = args.text("title")?;
    let link = issue_id(args)?;
    let project = api.project(args)?;
    if let Some(found) = keyed_cycle(api, project, key).await? {
        return Ok(json!({ "ref": reference(&found), "url": "" }));
    }
    let mut arguments = Map::new();
    arguments.insert("projectKey".to_owned(), json!(project));
    arguments.insert("name".to_owned(), json!(title));
    let description = with_marker(args.optional_text("description"), key);
    put_text(&mut arguments, "description", description.as_deref());
    let made = api.tool("zephyr_create_test_cycle", Value::Object(arguments)).await?;
    let cycle = text_of(&made, "/key");
    if cycle.is_empty() {
        return Err(ActionError::decode("the new test cycle has no key"));
    }
    if let Some(issue) = link {
        let arguments = json!({ "testCycleIdOrKey": cycle, "issueId": issue });
        api.tool("zephyr_create_test_cycle_issue_link", arguments).await?;
    }
    Ok(json!({ "ref": cycle, "url": "" }))
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

pub async fn record(api: &Api<'_>, args: Args<'_>, key: &str) -> Result<Value, ActionError> {
    let case = valid_key(args.text("case")?)?;
    let cycle = valid_key(args.text("cycle")?)?;
    let status = status_name(args.text("status")?)?;
    let project = args.optional_text("project").or(api.project.as_deref()).or_else(|| case_project(case));
    let project = valid_key(project.ok_or_else(|| ActionError::blocked(NO_PROJECT))?)?;
    if !key.is_empty() {
        let wanted = json!({ "projectKey": project, "testCycle": cycle, "testCase": case, "limit": PAGE });
        let marked = |execution: &Value| idempotency::carries_tag(&text_of(execution, "/comment"), key);
        if let Some(found) = by_cursor(api, "zephyr_get_test_executions", wanted, marked).await? {
            return Ok(json!({ "ref": reference(&found) }));
        }
    }
    let mut arguments = Map::new();
    arguments.insert("projectKey".to_owned(), json!(project));
    arguments.insert("testCaseKey".to_owned(), json!(case));
    arguments.insert("testCycleKey".to_owned(), json!(cycle));
    arguments.insert("statusName".to_owned(), json!(status));
    let comment = with_marker(args.optional_text("comment"), key);
    put_text(&mut arguments, "comment", comment.as_deref());
    if let Some(seconds) = args.0.get("seconds").and_then(Value::as_f64).filter(|seconds| *seconds >= 0.0) {
        arguments.insert("executionTime".to_owned(), json!((seconds * 1000.0).round() as u64));
    }
    let made = api.tool("zephyr_create_test_execution", Value::Object(arguments)).await?;
    Ok(json!({ "ref": reference(&made) }))
}
