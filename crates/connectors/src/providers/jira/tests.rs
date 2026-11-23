use domain::connection::ConnectionId;
use serde_json::json;

use super::*;
use crate::error::ErrorClass;
use crate::http::{FakeHttp, HttpResponse, Method};
use crate::idempotency;

const MCP: &str = "/v2/mcp";

fn creds() -> Credentials {
    let settings = json!({ "cloud_id": "cloud1", "site_url": "https://acme.atlassian.net/", "project": "BIG" });
    Credentials::new(&ConnectionId::from("jira1"), "at", settings)
}

fn session(results: &[Value]) -> FakeHttp {
    let init = HttpResponse::new(200, r#"{"jsonrpc":"2.0","id":1,"result":{}}"#).with_header("mcp-session-id", "s1");
    let mut http = FakeHttp::new().respond(Method::Post, MCP, init).once(Method::Post, MCP, 202, "");
    for (index, result) in results.iter().enumerate() {
        let content = json!({ "content": [{ "type": "text", "text": result.to_string() }] });
        let answer = json!({ "jsonrpc": "2.0", "id": index + 2, "result": content });
        http = http.once(Method::Post, MCP, 200, &answer.to_string());
    }
    http
}

fn tool_calls(http: &FakeHttp) -> Vec<(String, Value)> {
    http.sent()
        .iter()
        .map(|request| request.body_json())
        .filter(|body| body["method"] == "tools/call")
        .map(|body| {
            (body["params"]["name"].as_str().unwrap_or_default().to_owned(), body["params"]["arguments"].clone())
        })
        .collect()
}

async fn call(http: &FakeHttp, action: &str, args: Value, key: &str) -> Result<Value, ActionError> {
    Jira.call(http, &creds(), action, &args, key).await
}

#[test]
fn the_sign_in_is_the_configured_mcp_server_with_dynamic_registration() {
    let AuthSpec::Loopback(_) = Jira.descriptor().auth else { panic!("not loopback") };
    let flow = sign_in();
    assert_eq!(flow.server, brand::connectors::ATLASSIAN_MCP_URL);
    assert_eq!(flow.redirect, brand::connectors::OAUTH_REDIRECT);
    assert!(flow.scopes.contains(&"offline_access".to_owned()));
    assert_eq!(Jira.descriptor().name, "Jira");
    assert_eq!(Jira.presentation().description, "Sua conta Atlassian");
}

#[tokio::test]
async fn the_sites_come_from_the_accessible_resources_with_their_projects() {
    let http = session(&[
        json!([{"id": "c1", "url": "https://acme.atlassian.net", "name": "acme"}]),
        json!({"values": [{"key": "APP"}, {"key": "OPS"}]}),
    ]);
    let options = Jira.accounts(&http, &creds()).await;
    let Ok(options) = options else { panic!("{options:?}") };
    assert_eq!(options[0].id, "c1");
    assert_eq!(options[0].label, "acme.atlassian.net");
    assert_eq!(options[0].settings["cloud_id"], "c1");
    assert_eq!(options[0].settings["detail"], "Projetos APP, OPS");
    assert_eq!(http.sent()[0].header_value("authorization"), Some("Bearer at"));
    assert_eq!(tool_calls(&http)[1].1["cloudId"], "c1");
}

#[tokio::test]
async fn create_issue_finds_the_issue_already_labelled_with_the_key() {
    let http = session(&[json!({ "issues": [{ "id": "2", "key": "BIG-2" }] })]);
    let answer = call(&http, actions::CREATE_ISSUE, json!({"project": "BIG", "title": "Fix it!"}), "k1").await;
    assert_eq!(
        answer,
        Ok(json!({"id": "2", "key": "BIG-2", "url": "https://acme.atlassian.net/browse/BIG-2", "created": false}))
    );
    let calls = tool_calls(&http);
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].0, "searchJiraIssuesUsingJql");
    assert_eq!(calls[0].1["cloudId"], "cloud1");
    let jql = format!("project = \"BIG\" AND labels = \"{}\" ORDER BY created DESC", idempotency::label("k1"));
    assert_eq!(calls[0].1["jql"], jql);
}

#[tokio::test]
async fn create_issue_creates_with_the_key_as_a_label() {
    let http = session(&[json!({ "issues": [] }), json!({"id": "10", "key": "BIG-10"})]);
    let args = json!({"project": "BIG", "title": "T", "description": "**b**", "parent": "BIG-1", "labels": ["x"]});
    let answer = call(&http, actions::CREATE_ISSUE, args, "k2").await;
    assert_eq!(answer.map(|a| (a["key"].clone(), a["created"].clone())), Ok((json!("BIG-10"), json!(true))));
    let (name, sent) = tool_calls(&http)[1].clone();
    assert_eq!(name, "createJiraIssue");
    assert_eq!(sent["additional_fields"]["labels"], json!(["x", idempotency::label("k2")]));
    assert_eq!(sent["issueTypeName"], "Subtask");
    assert_eq!(sent["parent"], "BIG-1");
    assert_eq!(sent["contentFormat"], "adf");
    assert_eq!(sent["description"]["type"], "doc");
}

#[tokio::test]
async fn transition_to_the_current_status_is_already_done() {
    let http = session(&[json!({"key": "BIG-1", "fields": {"status": {"name": "Fazendo"}}})]);
    let answer = call(&http, actions::TRANSITION, json!({"issue": "BIG-1", "status": "fazendo"}), "k").await;
    assert_eq!(answer, Ok(json!({"status": "Fazendo", "changed": false})));
    assert_eq!(tool_calls(&http).len(), 1);
}

#[tokio::test]
async fn transition_picks_the_transition_that_reaches_the_named_status() {
    let http = session(&[
        json!({"fields": {"status": {"name": "A fazer"}}}),
        json!({"transitions": [{"id": "11", "to": {"name": "Fazendo"}}, {"id": "31", "to": {"name": "Feito"}}]}),
        json!(""),
    ]);
    let answer = call(&http, actions::TRANSITION, json!({"issue": "BIG-1", "status": "Feito"}), "k").await;
    assert_eq!(answer, Ok(json!({"status": "Feito", "changed": true})));
    let (name, sent) = tool_calls(&http)[2].clone();
    assert_eq!(name, "transitionJiraIssue");
    assert_eq!(sent["transition"], json!({"id": "31"}));
}

#[tokio::test]
async fn transition_without_a_path_is_permanent() {
    let http = session(&[json!({"fields": {"status": {"name": "A fazer"}}}), json!({"transitions": []})]);
    let answer = call(&http, actions::TRANSITION, json!({"issue": "BIG-1", "status": "Feito"}), "k").await;
    assert_eq!(answer.map_err(|e| e.class), Err(ErrorClass::Permanent));
}

#[tokio::test]
async fn a_comment_already_posted_is_not_posted_again() {
    let http = session(&[json!({"fields": {"comment": {"comments": [{"id": "7", "body": "Oi!"}]}}})]);
    let answer = call(&http, actions::COMMENT, json!({"issue": "BIG-1", "body": "oi"}), "k3").await;
    assert_eq!(answer, Ok(json!({"id": "7", "created": false})));
}

#[tokio::test]
async fn a_new_comment_is_sent_as_a_document() {
    let http = session(&[json!({"fields": {"comment": {"comments": []}}}), json!({"id": "8"})]);
    let answer = call(&http, actions::COMMENT, json!({"issue": "BIG-1", "body": "oi"}), "k4").await;
    assert_eq!(answer, Ok(json!({"id": "8", "created": true})));
    let (name, sent) = tool_calls(&http)[1].clone();
    assert_eq!(name, "addCommentToJiraIssue");
    let body: Value = serde_json::from_str(sent["commentBody"].as_str().unwrap_or_default()).unwrap_or_default();
    assert_eq!(body["content"][0]["content"][0]["text"], "oi");
}

#[tokio::test]
async fn set_impediment_flags_the_issue_and_explains_why() {
    let http = session(&[json!({}), json!({"fields": {"comment": {"comments": []}}}), json!({"id": "9"})]);
    let on =
        call(&http, actions::SET_IMPEDIMENT, json!({"issue": "BIG-1", "on": true, "reason": "falta acesso"}), "k5")
            .await;
    assert_eq!(on, Ok(json!({"impediment": true})));
    assert_eq!(tool_calls(&http)[0].1["fields"], json!({DEFAULT_FLAG_FIELD: [{"value": "Impediment"}]}));
    let http = session(&[json!({})]);
    let off = call(&http, actions::SET_IMPEDIMENT, json!({"issue": "BIG-1", "on": false}), "k6").await;
    assert_eq!(off, Ok(json!({"impediment": false})));
    assert_eq!(tool_calls(&http)[0].1["fields"], json!({DEFAULT_FLAG_FIELD: null}));
}

#[tokio::test]
async fn statuses_are_the_distinct_statuses_seen_and_reachable() {
    let status = |id: &str, name: &str| json!({"id": id, "name": name, "statusCategory": {"key": "new"}});
    let http = session(&[
        json!({"issues": [{"key": "BIG-1", "fields": {"status": status("1", "A fazer")}}, {"key": "BIG-2", "fields": {"status": status("1", "A fazer")}}]}),
        json!({"transitions": [{"id": "5", "to": status("3", "Feito")}]}),
    ]);
    let listed = call(&http, actions::STATUSES, json!({"project": "BIG"}), "").await;
    let names =
        listed.map(|l| l["statuses"].as_array().map(|s| s.iter().map(|x| x["name"].clone()).collect::<Vec<_>>()));
    assert_eq!(names, Ok(Some(vec![json!("A fazer"), json!("Feito")])));
}

#[tokio::test]
async fn an_issue_reads_description_subtasks_comments_and_the_flag() {
    let answer = json!({ "key": "BIG-1", "fields": {
        "summary": "T", "status": {"name": "Fazendo"}, "labels": [],
        "description": {"type": "doc", "version": 1, "content": [{"type": "paragraph", "content": [{"type": "text", "text": "d"}]}]},
        "subtasks": [{"key": "BIG-2", "fields": {"summary": "s", "status": {"name": "Feito"}}}],
        "attachment": [{"filename": "a.png", "size": 3, "content": "https://f"}],
        "comment": {"comments": [{"id": "5", "author": {"displayName": "Ana"}, "body": "texto", "created": "2024-01-01T00:00:00.000+0000"}]},
        DEFAULT_FLAG_FIELD: [{"value": "Impediment"}],
    }});
    let http = session(&[answer]);
    let read = call(&http, actions::ISSUE, json!({"issue": "BIG-1"}), "").await;
    let Ok(read) = read else { panic!("{read:?}") };
    assert_eq!(read["description"], "d");
    assert_eq!(read["impediment"], true);
    assert_eq!(read["subtasks"][0]["status"], "Feito");
    assert_eq!(read["comments"][0]["body"], "texto");
    assert_eq!(read["attachments"][0]["name"], "a.png");
    assert_eq!(tool_calls(&http)[0].1["responseContentFormat"], "markdown");
}

#[tokio::test]
async fn a_rejected_token_is_an_auth_error_and_a_bad_key_never_reaches_a_tool() {
    let http = FakeHttp::new().once(Method::Post, MCP, 401, r#"{"error":"invalid_token"}"#);
    let refused = call(&http, actions::ISSUE, json!({"issue": "BIG-1"}), "").await;
    assert_eq!(refused.map_err(|e| e.class), Err(ErrorClass::Auth));
    let http = session(&[]);
    let injected = call(&http, actions::ISSUE, json!({"issue": "BIG-1\" OR 1=1"}), "").await;
    assert_eq!(injected.map_err(|e| e.class), Err(ErrorClass::Permanent));
    assert!(tool_calls(&http).is_empty());
}

#[tokio::test]
async fn the_first_poll_starts_the_cursor_and_later_polls_read_changes_since_it() {
    let source = Jira.events().unwrap_or_else(|| panic!("no events"));
    let http = FakeHttp::new();
    let first = source.poll(&http, &creds(), None).await;
    assert!(first.as_ref().is_ok_and(|b| b.events.is_empty() && !b.cursor.is_empty()));
    assert!(http.sent().is_empty());
    let page =
        json!({ "isLast": true, "issues": [{"key": "BIG-1", "fields": {"updated": "2024-01-02T10:00:00.000+0000"}}]});
    let detail = json!({
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
    });
    let http = session(&[page, detail]);
    let batch = source.poll(&http, &creds(), Some("2024-01-02T00:00:00Z")).await;
    let Ok(batch) = batch else { panic!("{batch:?}") };
    let kinds: Vec<&str> = batch.events.iter().map(|e| e.kind.as_str()).collect();
    assert_eq!(kinds, ["tracker.comment.added", "tracker.status.changed"]);
    assert_eq!(batch.events[0].data["body"], "novo");
    assert_eq!(batch.events[1].data["to"], "Feito");
    assert_eq!(batch.events[1].id, "jira:BIG-1:status:h1");
    assert_eq!(batch.cursor, "2024-01-02T10:00:00Z");
    let calls = tool_calls(&http);
    assert_eq!(calls[0].1["jql"], "project = \"BIG\" AND updated >= \"2024-01-01 09:00\" ORDER BY updated ASC");
    assert_eq!(calls[1].1["expand"], "changelog");
}
