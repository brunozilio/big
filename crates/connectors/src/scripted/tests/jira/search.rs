use serde_json::{Value, json};

use super::{call, session, tool_calls};

fn issue(key: &str, kind: &str, subtask: bool) -> Value {
    json!({"key": key, "fields": {"summary": key, "status": {"name": "A fazer"}, "issuetype": {"name": kind, "subtask": subtask}}})
}

fn refs(found: &Value) -> Vec<(String, String, bool)> {
    let listed = found["issues"].as_array().cloned().unwrap_or_default();
    listed
        .iter()
        .map(|issue| {
            let text = |name: &str| issue[name].as_str().unwrap_or_default().to_owned();
            (text("ref"), text("type"), issue["sprint"].as_bool().unwrap_or(false))
        })
        .collect()
}

#[tokio::test]
async fn a_full_key_is_read_directly_and_marked_when_it_is_in_an_open_sprint() {
    let http = session(&[issue("BIG-7", "História", false), json!({"issues": [{"key": "BIG-7"}]})]);
    let found = call(&http, "tracker.search", json!({"query": " big-7 ", "pick": true}), "").await;
    let Ok(found) = found else { panic!("{found:?}") };
    assert_eq!(refs(&found), vec![("BIG-7".to_owned(), "História".to_owned(), true)]);
    let calls = tool_calls(&http);
    assert_eq!(calls[0].0, "getJiraIssue");
    assert_eq!(calls[0].1["issueIdOrKey"], "BIG-7");
    assert_eq!(calls[1].1["jql"], "key in (\"BIG-7\") AND sprint in openSprints()");
    assert_eq!(calls.len(), 2);
}

#[tokio::test]
async fn picking_keeps_only_the_configured_types_and_never_subtasks() {
    let page = json!({"isLast": true, "issues": [
        issue("BIG-1", "Story", false), issue("BIG-2", "Task", false), issue("BIG-3", "Bug", true),
        issue("BIG-4", "Defeito", false), issue("BIG-5", "Spike", false), issue("BIG-6", "Epic", false)
    ]});
    let http = session(&[page, json!({"issues": [{"key": "BIG-5"}]})]);
    let found = call(&http, "tracker.search", json!({"query": "login", "pick": true}), "").await;
    let Ok(found) = found else { panic!("{found:?}") };
    let kept = vec![
        ("BIG-1".to_owned(), "Story".to_owned(), false),
        ("BIG-4".to_owned(), "Defeito".to_owned(), false),
        ("BIG-5".to_owned(), "Spike".to_owned(), true),
    ];
    assert_eq!(refs(&found), kept);
    let calls = tool_calls(&http);
    let wanted = "project = \"BIG\" AND text ~ \"login\" AND issuetype not in subTaskIssueTypes() ORDER BY updated DESC";
    assert_eq!(calls[0].1["jql"], wanted);
}

#[tokio::test]
async fn a_bare_number_reads_the_key_in_the_project_and_also_searches_the_text() {
    let page = json!({"isLast": true, "issues": [issue("BIG-42", "Story", false), issue("BIG-9", "Bug", false)]});
    let http = session(&[issue("BIG-42", "Story", false), page, json!({"issues": []})]);
    let found = call(&http, "tracker.search", json!({"query": "42", "pick": true}), "").await;
    let Ok(found) = found else { panic!("{found:?}") };
    let kept = vec![("BIG-42".to_owned(), "Story".to_owned(), false), ("BIG-9".to_owned(), "Bug".to_owned(), false)];
    assert_eq!(refs(&found), kept);
    assert_eq!(tool_calls(&http)[0].1["issueIdOrKey"], "BIG-42");
}

#[tokio::test]
async fn a_key_that_does_not_exist_finds_nothing_instead_of_failing() {
    let http = session(&[]);
    let found = call(&http, "tracker.search", json!({"query": "BIG-999", "pick": true}), "").await;
    assert_eq!(found, Ok(json!({"issues": [], "cursor": null})));
}
