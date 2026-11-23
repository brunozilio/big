use domain::connection::ConnectionId;
use serde_json::json;

use super::*;
use crate::error::ErrorClass;
use crate::http::{FakeHttp, Method};
use crate::idempotency;

fn creds() -> Credentials {
    let settings = json!({ "cloud_id": "cloud1", "site_url": "https://acme.atlassian.net/", "project": "BIG" });
    Credentials::new(&ConnectionId::from("jira1"), "at", settings)
}

async fn call(http: &FakeHttp, action: &str, args: Value, key: &str) -> Result<Value, ActionError> {
    Jira.call(http, &creds(), action, &args, key).await
}

fn property(key: &str) -> Value {
    json!({ idempotency::property_key(): { "key": key } })
}

#[test]
fn the_oauth_app_is_atlassian_three_legged_with_the_configured_redirect() {
    let AuthSpec::Redirect(app) = Jira.descriptor().auth else { panic!("not redirect") };
    assert_eq!(app.redirect, brand::connectors::OAUTH_REDIRECT);
    assert!(app.scopes.contains(&"offline_access".to_owned()));
    assert_eq!(app.token_body, TokenBody::Json);
}

#[tokio::test]
async fn the_sites_come_from_the_accessible_resources() {
    let http = FakeHttp::new().once(
        Method::Get,
        "/oauth/token/accessible-resources",
        200,
        r#"[{"id":"c1","url":"https://acme.atlassian.net","name":"acme"}]"#,
    );
    let options = Jira.accounts(&http, &creds()).await;
    let Ok(options) = options else { panic!("{options:?}") };
    assert_eq!(options[0].id, "c1");
    assert_eq!(options[0].settings["cloud_id"], "c1");
    assert_eq!(http.sent()[0].header_value("authorization"), Some("Bearer at"));
}

#[tokio::test]
async fn create_issue_finds_the_issue_already_marked_with_the_key() {
    let found = json!({ "issues": [
        { "id": "1", "key": "BIG-1", "properties": property("other") },
        { "id": "2", "key": "BIG-2", "properties": property("k1") },
    ]});
    let http = FakeHttp::new().once(Method::Post, "/search/jql", 200, &found.to_string());
    let answer = call(&http, actions::CREATE_ISSUE, json!({"title": "Fix it!"}), "k1").await;
    assert_eq!(
        answer,
        Ok(json!({"id": "2", "ref": "BIG-2", "url": "https://acme.atlassian.net/browse/BIG-2", "title": "Fix it!", "status": "", "created": false}))
    );
    let search = http.sent()[0].body_json();
    assert_eq!(search["jql"], "project = \"BIG\" AND summary ~ \"Fix it\" ORDER BY created DESC");
    assert_eq!(search["properties"], json!([idempotency::property_key()]));
    assert!(http.sent_to(Method::Post, "/issue").iter().all(|r| !r.url.ends_with("/issue")));
}

#[tokio::test]
async fn create_issue_creates_with_the_key_as_an_entity_property() {
    let http = FakeHttp::new().once(Method::Post, "/search/jql", 200, r#"{"issues":[]}"#).once(
        Method::Post,
        "/rest/api/3/issue",
        201,
        r#"{"id":"10","key":"BIG-10"}"#,
    );
    let args = json!({"project": "BIG", "title": "T", "description": "**b**", "parent": "BIG-1", "labels": ["x"]});
    let answer = call(&http, actions::CREATE_ISSUE, args, "k2").await;
    assert_eq!(answer.map(|a| (a["ref"].clone(), a["created"].clone())), Ok((json!("BIG-10"), json!(true))));
    let sent = http.sent()[1].body_json();
    assert_eq!(sent["properties"][0]["key"], idempotency::property_key());
    assert_eq!(sent["properties"][0]["value"]["key"], "k2");
    assert_eq!(sent["fields"]["issuetype"]["name"], "Subtask");
    assert_eq!(sent["fields"]["parent"]["key"], "BIG-1");
    assert_eq!(sent["fields"]["description"]["type"], "doc");
    assert!(http.sent()[1].url.starts_with("https://api.atlassian.com/ex/jira/cloud1/rest/api/3/issue"));
}

#[tokio::test]
async fn transition_to_the_current_status_is_already_done() {
    let http = FakeHttp::new().once(
        Method::Get,
        "/issue/BIG-1?fields=status",
        200,
        r#"{"fields":{"status":{"name":"Fazendo"}}}"#,
    );
    let answer = call(&http, actions::TRANSITION, json!({"issue": "BIG-1", "status": "fazendo"}), "k").await;
    assert_eq!(answer, Ok(json!({"ref": "BIG-1", "status": "Fazendo", "changed": false})));
    assert_eq!(http.sent().len(), 1);
}

#[tokio::test]
async fn transition_picks_the_transition_that_reaches_the_named_status() {
    let transitions = r#"{"transitions":[{"id":"11","to":{"name":"Fazendo"}},{"id":"31","to":{"name":"Feito"}}]}"#;
    let http = FakeHttp::new()
        .once(Method::Get, "/issue/BIG-1?fields=status", 200, r#"{"fields":{"status":{"name":"A fazer"}}}"#)
        .once(Method::Get, "/issue/BIG-1/transitions", 200, transitions)
        .once(Method::Post, "/issue/BIG-1/transitions", 204, "");
    let answer = call(&http, actions::TRANSITION, json!({"issue": "BIG-1", "status": "Feito"}), "k").await;
    assert_eq!(answer, Ok(json!({"ref": "BIG-1", "status": "Feito", "changed": true})));
    assert_eq!(http.sent()[2].body_json(), json!({"transition": {"id": "31"}}));
}

#[tokio::test]
async fn transition_without_a_path_is_permanent() {
    let http = FakeHttp::new()
        .once(Method::Get, "/issue/BIG-1?fields=status", 200, r#"{"fields":{"status":{"name":"A fazer"}}}"#)
        .once(Method::Get, "/issue/BIG-1/transitions", 200, r#"{"transitions":[]}"#);
    let answer = call(&http, actions::TRANSITION, json!({"issue": "BIG-1", "status": "Feito"}), "k").await;
    assert_eq!(answer.map_err(|e| e.class), Err(ErrorClass::Permanent));
}

#[tokio::test]
async fn a_comment_already_marked_with_the_key_is_not_posted_again() {
    let listed = json!({ "comments": [{ "id": "7", "properties": [{ "key": idempotency::property_key(), "value": { "key": "k3" } }] }] });
    let http = FakeHttp::new().once(Method::Get, "/issue/BIG-1/comment?expand=properties", 200, &listed.to_string());
    let answer = call(&http, actions::COMMENT, json!({"issue": "BIG-1", "body": "oi"}), "k3").await;
    assert_eq!(answer.map(|a| (a["id"].clone(), a["body"].clone(), a["created"].clone())), Ok((json!("7"), json!("oi"), json!(false))));
}

#[tokio::test]
async fn a_new_comment_carries_the_invisible_key() {
    let http = FakeHttp::new()
        .once(Method::Get, "/issue/BIG-1/comment?expand=properties", 200, r#"{"comments":[]}"#)
        .once(Method::Post, "/issue/BIG-1/comment", 201, r#"{"id":"8"}"#);
    let answer = call(&http, actions::COMMENT, json!({"issue": "BIG-1", "body": "oi"}), "k4").await;
    assert_eq!(answer.map(|a| (a["id"].clone(), a["created"].clone())), Ok((json!("8"), json!(true))));
    let sent = http.sent()[1].body_json();
    assert_eq!(sent["properties"][0]["value"]["key"], "k4");
    assert_eq!(sent["body"]["content"][0]["content"][0]["text"], "oi");
}

#[tokio::test]
async fn set_impediment_flags_the_issue_and_explains_why() {
    let http = FakeHttp::new()
        .once(Method::Put, "/issue/BIG-1", 204, "")
        .once(Method::Get, "/issue/BIG-1/comment", 200, r#"{"comments":[]}"#)
        .once(Method::Post, "/issue/BIG-1/comment", 201, r#"{"id":"9"}"#)
        .once(Method::Put, "/issue/BIG-1", 204, "");
    let on =
        call(&http, actions::SET_IMPEDIMENT, json!({"issue": "BIG-1", "on": true, "reason": "falta acesso"}), "k5")
            .await;
    assert_eq!(on, Ok(json!({"on": true, "reason": "falta acesso"})));
    assert_eq!(http.sent()[0].body_json(), json!({"fields": {DEFAULT_FLAG_FIELD: [{"value": "Impediment"}]}}));
    let off = call(&http, actions::SET_IMPEDIMENT, json!({"issue": "BIG-1", "on": false}), "k6").await;
    assert_eq!(off, Ok(json!({"on": false, "reason": null})));
    assert_eq!(http.sent()[3].body_json(), json!({"fields": {DEFAULT_FLAG_FIELD: null}}));
}

#[tokio::test]
async fn statuses_are_the_distinct_statuses_of_the_project() {
    let answer = r#"[{"name":"Task","statuses":[{"id":"1","name":"A fazer","statusCategory":{"key":"new"}},{"id":"3","name":"Feito","statusCategory":{"key":"done"}}]},
                     {"name":"Bug","statuses":[{"id":"1","name":"A fazer","statusCategory":{"key":"new"}}]}]"#;
    let http = FakeHttp::new().once(Method::Get, "/project/BIG/statuses", 200, answer);
    let listed = call(&http, actions::STATUSES, json!({}), "").await;
    let names = listed.map(|l| l.as_array().map(|s| s.iter().map(|x| x["name"].clone()).collect::<Vec<_>>()));
    assert_eq!(names, Ok(Some(vec![json!("A fazer"), json!("Feito")])));
}

#[tokio::test]
async fn an_issue_reads_description_subtasks_comments_and_the_flag() {
    let answer = json!({ "key": "BIG-1", "fields": {
        "summary": "T", "status": {"name": "Fazendo"}, "labels": [],
        "description": {"type": "doc", "version": 1, "content": [{"type": "paragraph", "content": [{"type": "text", "text": "d"}]}]},
        "subtasks": [{"key": "BIG-2", "fields": {"summary": "s", "status": {"name": "Feito", "statusCategory": {"key": "done"}}}}],
        "attachment": [{"filename": "a.png", "size": 3, "content": "https://f"}],
        "comment": {"comments": [{"id": "5", "author": {"displayName": "Ana"}, "body": "texto", "created": "2024-01-01T00:00:00.000+0000"}]},
        DEFAULT_FLAG_FIELD: [{"value": "Impediment"}],
    }});
    let http = FakeHttp::new().once(Method::Get, "/issue/BIG-1?fields=", 200, &answer.to_string());
    let read = call(&http, actions::ISSUE, json!({"issue": "BIG-1"}), "").await;
    let Ok(read) = read else { panic!("{read:?}") };
    assert_eq!(read["description"], "d");
    assert_eq!(read["ref"], "BIG-1");
    assert_eq!(read["impediment"], json!({"on": true, "reason": null}));
    assert_eq!(read["subtasks"][0]["status"], "Feito");
    assert_eq!(read["subtasks"][0]["done"], true);
    assert_eq!(read["comments"][0]["author"]["name"], "Ana");
    assert_eq!(read["attachments"][0]["name"], "a.png");
}

#[tokio::test]
async fn a_rejected_token_is_an_auth_error_and_a_bad_key_never_reaches_the_api() {
    let http = FakeHttp::new().once(Method::Get, "/issue/BIG-1", 401, "{}");
    let refused = call(&http, actions::ISSUE, json!({"issue": "BIG-1"}), "").await;
    assert_eq!(refused.map_err(|e| e.class), Err(ErrorClass::Auth));
    let injected = call(&http, actions::ISSUE, json!({"issue": "BIG-1\" OR 1=1"}), "").await;
    assert_eq!(injected.map_err(|e| e.class), Err(ErrorClass::Permanent));
    assert_eq!(http.sent().len(), 1);
}

#[tokio::test]
async fn the_first_poll_starts_the_cursor_and_later_polls_read_changes_since_it() {
    let source = Jira.events().unwrap_or_else(|| panic!("no events"));
    let http = FakeHttp::new();
    let first = source.poll(&http, &creds(), None).await;
    assert!(first.as_ref().is_ok_and(|b| b.events.is_empty() && !b.cursor.is_empty()));
    assert!(http.sent().is_empty());
    let page = json!({ "isLast": true, "issues": [{
        "key": "BIG-1",
        "fields": {
            "summary": "T", "status": {"name": "Feito"},
            "created": "2023-12-01T00:00:00.000+0000", "updated": "2024-01-02T10:00:00.000+0000",
            "comment": {"comments": [
                {"id": "1", "created": "2023-12-31T00:00:00.000+0000", "body": "velho"},
                {"id": "2", "created": "2024-01-02T09:00:00.000+0000", "author": {"displayName": "Ana"}, "body": "novo"}
            ]}
        },
        "changelog": {"histories": [{"id": "h1", "created": "2024-01-02T10:00:00.000+0000", "author": {"displayName": "Ana"},
            "items": [{"field": "status", "fromString": "Fazendo", "toString": "Feito"}, {"field": "labels"}]}]}
    }]});
    let http = FakeHttp::new().once(Method::Post, "/search/jql", 200, &page.to_string());
    let batch = source.poll(&http, &creds(), Some("2024-01-02T00:00:00Z")).await;
    let Ok(batch) = batch else { panic!("{batch:?}") };
    let kinds: Vec<&str> = batch.events.iter().map(|e| e.kind.as_str()).collect();
    assert_eq!(kinds, ["tracker.comment.added", "tracker.status.changed"]);
    assert_eq!(batch.events[1].data["to"], "Feito");
    assert_eq!(batch.events[1].id, "jira:BIG-1:status:h1");
    assert_eq!(batch.events[1].subject, "BIG-1");
    assert_eq!(batch.cursor, "2024-01-02T10:00:00Z");
    let jql = http.sent()[0].body_json()["jql"].clone();
    assert_eq!(jql, "project = \"BIG\" AND updated >= \"2024-01-01 09:00\" ORDER BY updated ASC");
}
