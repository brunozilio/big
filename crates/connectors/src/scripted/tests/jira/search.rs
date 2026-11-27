use serde_json::{Value, json};

use super::{call, session, tool_calls};

fn issue(key: &str, kind: &str, level: i64) -> Value {
    let typed = json!({"name": kind, "subtask": level == -1, "hierarchyLevel": level});
    json!({"key": key, "fields": {"summary": key, "status": {"name": "A fazer"}, "issuetype": typed}})
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
    let http = session(&[issue("BIG-7", "História", 0), json!({"issues": [{"key": "BIG-7"}]})]);
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
async fn picking_keeps_only_the_root_work_items_never_epics_or_subtasks() {
    let page = json!({"isLast": true, "issues": [
        issue("BIG-1", "Story", 0), issue("BIG-2", "Task", 0), issue("BIG-3", "Bug", -1),
        issue("BIG-4", "Defeito", 0), issue("BIG-5", "Spike", 0), issue("BIG-6", "Epic", 1)
    ]});
    let http = session(&[page, json!({"issues": [{"key": "BIG-5"}]})]);
    let found = call(&http, "tracker.search", json!({"query": "login", "pick": true}), "").await;
    let Ok(found) = found else { panic!("{found:?}") };
    let kept = vec![
        ("BIG-1".to_owned(), "Story".to_owned(), false),
        ("BIG-2".to_owned(), "Task".to_owned(), false),
        ("BIG-4".to_owned(), "Defeito".to_owned(), false),
        ("BIG-5".to_owned(), "Spike".to_owned(), true),
    ];
    assert_eq!(refs(&found), kept);
    let calls = tool_calls(&http);
    let wanted =
        "project = \"BIG\" AND text ~ \"login\" AND issuetype not in subTaskIssueTypes() ORDER BY updated DESC";
    assert_eq!(calls[0].1["jql"], wanted);
}

#[tokio::test]
async fn a_bare_number_reads_the_key_in_the_project_and_also_searches_the_text() {
    let page = json!({"isLast": true, "issues": [issue("BIG-42", "Story", 0), issue("BIG-9", "Bug", 0)]});
    let http = session(&[issue("BIG-42", "Story", 0), page, json!({"issues": []})]);
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

#[tokio::test]
async fn a_key_typed_with_spaces_or_another_dash_is_still_read_as_the_key() {
    for typed in [
        "FIRP25XPTO - 2532",
        "firp25xpto–2532",
        " FIRP25XPTO -2532 ",
        "https://acme.atlassian.net/browse/FIRP25XPTO-2532",
    ] {
        let http = session(&[issue("FIRP25XPTO-2532", "Story", 0), json!({"issues": []})]);
        let found = call(&http, "tracker.search", json!({"query": typed, "pick": true}), "").await;
        let Ok(found) = found else { panic!("{typed}: {found:?}") };
        assert_eq!(refs(&found), vec![("FIRP25XPTO-2532".to_owned(), "Story".to_owned(), false)], "{typed}");
        let calls = tool_calls(&http);
        assert_eq!(calls[0].1["issueIdOrKey"], "FIRP25XPTO-2532", "{typed}");
        assert_eq!(calls.len(), 2, "{typed}");
    }
}

#[tokio::test]
async fn a_key_and_number_split_by_a_space_is_tried_as_a_key_and_as_text() {
    let page = json!({"isLast": true, "issues": []});
    let http = session(&[issue("FIRP25XPTO-2532", "Bug", 0), page, json!({"issues": []})]);
    let found = call(&http, "tracker.search", json!({"query": "firp25xpto 2532", "pick": true}), "").await;
    let Ok(found) = found else { panic!("{found:?}") };
    assert_eq!(refs(&found), vec![("FIRP25XPTO-2532".to_owned(), "Bug".to_owned(), false)]);
    assert_eq!(tool_calls(&http)[0].1["issueIdOrKey"], "FIRP25XPTO-2532");
}

#[tokio::test]
async fn the_exact_key_shows_whatever_its_type_but_never_as_a_subtask() {
    let http = session(&[issue("BIG-8", "Tarefa", 0), json!({"issues": []})]);
    let found = call(&http, "tracker.search", json!({"query": "BIG-8", "pick": true}), "").await;
    let Ok(found) = found else { panic!("{found:?}") };
    assert_eq!(refs(&found), vec![("BIG-8".to_owned(), "Tarefa".to_owned(), false)]);
    let http = session(&[issue("BIG-9", "Subtarefa", -1)]);
    let found = call(&http, "tracker.search", json!({"query": "BIG-9", "pick": true}), "").await;
    assert_eq!(found, Ok(json!({"issues": [], "cursor": null})));
}

#[tokio::test]
async fn when_the_issue_read_answers_something_else_the_key_is_found_by_jql() {
    let http = session(&[
        json!({"errorMessages": ["x"]}),
        json!({"issues": [issue("BIG-5", "Story", 0)]}),
        json!({"issues": []}),
    ]);
    let found = call(&http, "tracker.search", json!({"query": "BIG-5", "pick": true}), "").await;
    let Ok(found) = found else { panic!("{found:?}") };
    assert_eq!(refs(&found), vec![("BIG-5".to_owned(), "Story".to_owned(), false)]);
    assert_eq!(tool_calls(&http)[1].1["jql"], "key = \"BIG-5\"");
}

#[tokio::test]
async fn without_a_level_on_the_issue_the_project_types_say_which_are_root() {
    let bare = |key: &str, kind: &str| json!({"key": key, "fields": {"summary": key, "status": {"name": "A fazer"}, "issuetype": {"name": kind}}});
    let page = json!({"isLast": true, "issues": [bare("APP-1", "Épico"), bare("APP-2", "História")]});
    let types =
        json!({"issueTypes": [{"name": "Épico", "hierarchyLevel": 1}, {"name": "História", "hierarchyLevel": 0}]});
    let http = session(&[page, types, json!({"issues": []})]);
    let found = call(&http, "tracker.search", json!({"query": "login", "project": "APP", "pick": true}), "").await;
    let Ok(found) = found else { panic!("{found:?}") };
    assert_eq!(refs(&found), vec![("APP-2".to_owned(), "História".to_owned(), false)]);
}
