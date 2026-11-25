use domain::connection::ConnectionId;
use serde_json::json;

use super::fake::AtlassianV2;
use super::*;
use crate::error::ErrorClass;
use crate::http::{FakeHttp, Method};
use crate::idempotency;

const MCP: &str = "/v2/mcp";

fn creds() -> Credentials {
    let settings = json!({ "cloud_id": "cloud1", "site_url": "https://acme.atlassian.net/", "project": "BIG" });
    Credentials::new(&ConnectionId::from("jira1"), "at", settings)
}

fn session(results: &[Value]) -> AtlassianV2 {
    AtlassianV2::new(results)
}

fn structured(results: &[Value]) -> AtlassianV2 {
    AtlassianV2::structured(results)
}

fn real_sites() -> Value {
    json!([{
        "id": "0aeb9d87-1111", "url": "https://minhaempresa.atlassian.net", "name": "minhaempresa",
        "scopes": ["read:jira-work", "write:jira-work"], "avatarUrl": "https://a/x.png",
    }])
}

fn tool_calls(http: &AtlassianV2) -> Vec<(String, Value)> {
    http.calls()
}

async fn call(http: &dyn Http, action: &str, args: Value, key: &str) -> Result<Value, ActionError> {
    let answer = Jira.call(http, &creds(), action, &args, key).await;
    if let Ok(value) = &answer {
        let spec = Jira.descriptor().action(action).cloned().unwrap_or_else(|| panic!("no {action}"));
        assert_eq!(crate::descriptor::conforms(&spec.output_schema, value), Ok(()), "{action}: {value}");
    }
    answer
}

#[test]
fn the_sign_in_is_the_configured_mcp_server_with_dynamic_registration() {
    let AuthSpec::Loopback(_) = Jira.descriptor().auth else { panic!("not loopback") };
    let flow = sign_in();
    assert_eq!(flow.server, brand::connectors::ATLASSIAN_MCP_URL);
    assert_eq!(flow.redirect, brand::connectors::OAUTH_REDIRECT);
    assert!(flow.scopes.contains(&"offline_access".to_owned()));
    assert_eq!(Jira.descriptor().name, "Jira");
    assert_eq!(Jira.presentation(domain::Language::Portuguese).description, "Sua conta Atlassian");
    assert_eq!(Jira.presentation(domain::Language::English).description, "Your Atlassian account");
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
    let detail = crate::texts::detail(domain::Language::Portuguese, &options[0].settings);
    assert_eq!(detail.as_deref(), Some("Projetos APP, OPS"));
    let detail = crate::texts::detail(domain::Language::English, &options[0].settings);
    assert_eq!(detail.as_deref(), Some("Projects APP, OPS"));
    assert_eq!(http.sent()[0].header_value("authorization"), Some("Bearer at"));
    let (name, sent) = tool_calls(&http)[1].clone();
    assert_eq!(
        (name.as_str(), &sent["cloudId"], &sent["name"]),
        ("executeRead", &json!("c1"), &json!("listJiraProjects"))
    );
}

#[tokio::test]
async fn create_issue_finds_the_issue_already_labelled_with_the_key() {
    let found = json!({ "issues": [{ "id": "2", "key": "BIG-2", "fields": {"summary": "Fix it!", "status": {"name": "A fazer"}} }] });
    let http = session(&[found]);
    let answer = call(&http, actions::CREATE_ISSUE, json!({"title": "Fix it!"}), "k1").await;
    assert_eq!(
        answer,
        Ok(
            json!({"ref": "BIG-2", "title": "Fix it!", "status": "A fazer", "url": "https://acme.atlassian.net/browse/BIG-2", "created": false})
        )
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
    assert_eq!(
        answer.map(|a| (a["ref"].clone(), a["title"].clone(), a["created"].clone())),
        Ok((json!("BIG-10"), json!("T"), json!(true)))
    );
    let (name, sent) = tool_calls(&http)[1].clone();
    assert_eq!(name, "createJiraIssue");
    assert_eq!(sent["labels"], json!(["x", idempotency::label("k2")]));
    assert_eq!(sent["issueType"], "Subtask");
    assert_eq!(sent["parent"], "BIG-1");
    assert_eq!(sent["contentFormat"], "markdown");
    assert_eq!(sent["description"], "**b**");
}

#[tokio::test]
async fn transition_to_the_current_status_is_already_done() {
    let http = session(&[json!({"key": "BIG-1", "fields": {"status": {"name": "Fazendo"}}})]);
    let answer = call(&http, actions::TRANSITION, json!({"issue": "BIG-1", "status": "fazendo"}), "k").await;
    assert_eq!(answer, Ok(json!({"ref": "BIG-1", "status": "Fazendo", "changed": false})));
    assert_eq!(tool_calls(&http).len(), 1);
}

#[tokio::test]
async fn transition_picks_the_transition_that_reaches_the_named_status() {
    let http = session(&[
        json!({"fields": {"status": {"name": "A fazer"}}}),
        json!({"transitions": [{"id": "11", "to": {"name": "Fazendo"}}, {"id": 31, "to": {"name": "Feito"}}]}),
        json!({"key": "BIG-1", "status": {"name": "Feito"}}),
    ]);
    let answer = call(&http, actions::TRANSITION, json!({"issue": "BIG-1", "status": "Feito"}), "k").await;
    assert_eq!(answer, Ok(json!({"ref": "BIG-1", "status": "Feito", "changed": true})));
    let calls = tool_calls(&http);
    assert_eq!(calls[1].0, "executeRead");
    assert_eq!(calls[1].1["name"], "listJiraIssueTransitions");
    assert_eq!(calls[1].1["inputs"], json!({"issueIdOrKey": "BIG-1"}));
    let (name, sent) = calls[2].clone();
    assert_eq!(name, "transitionJiraIssue");
    assert_eq!(sent["transitionId"], "31");
    assert_eq!(sent["cloudId"], "cloud1");
}

#[tokio::test]
async fn transition_without_a_path_is_permanent() {
    let http = session(&[json!({"fields": {"status": {"name": "A fazer"}}}), json!({"transitions": []})]);
    let answer = call(&http, actions::TRANSITION, json!({"issue": "BIG-1", "status": "Feito"}), "k").await;
    assert_eq!(answer.map_err(|e| e.class), Err(ErrorClass::Permanent));
}

#[tokio::test]
async fn a_comment_already_posted_is_not_posted_again() {
    let known = json!({"id": "7", "body": "Oi!", "author": {"accountId": "a1", "displayName": "Ana"}, "created": "2024-01-01T00:00:00.000+0000"});
    let http = session(&[json!({"comments": [known], "isLast": true, "total": 1})]);
    let answer = call(&http, actions::COMMENT, json!({"issue": "BIG-1", "body": "oi"}), "k3").await;
    assert_eq!(
        answer,
        Ok(
            json!({"id": "7", "author": {"id": "a1", "name": "Ana"}, "body": "Oi!", "at": "2024-01-01T00:00:00.000+0000", "created": false})
        )
    );
}

#[tokio::test]
async fn a_new_comment_is_sent_as_markdown() {
    let http = session(&[json!({"comments": [], "isLast": true}), json!({"commentId": "8", "body": "oi"})]);
    let answer = call(&http, actions::COMMENT, json!({"issue": "BIG-1", "body": "oi"}), "k4").await;
    assert_eq!(answer, Ok(json!({"id": "8", "author": null, "body": "oi", "at": "", "created": true})));
    let calls = tool_calls(&http);
    assert_eq!(calls[0].0, "listJiraIssueComments");
    let (name, sent) = calls[1].clone();
    assert_eq!(name, "addOrEditJiraIssueComment");
    assert_eq!((&sent["commentBody"], &sent["contentFormat"]), (&json!("oi"), &json!("markdown")));
}

#[tokio::test]
async fn set_impediment_flags_the_issue_and_explains_why() {
    let http = session(&[json!({}), json!({"comments": []}), json!({"commentId": "9"})]);
    let on =
        call(&http, actions::SET_IMPEDIMENT, json!({"issue": "BIG-1", "on": true, "reason": "falta acesso"}), "k5")
            .await;
    assert_eq!(on, Ok(json!({"on": true, "reason": "falta acesso"})));
    let names: Vec<String> = tool_calls(&http).into_iter().map(|(name, _)| name).collect();
    assert_eq!(names, ["editJiraIssue", "listJiraIssueComments", "addOrEditJiraIssueComment"]);
    assert_eq!(tool_calls(&http)[0].1["fields"], json!({DEFAULT_FLAG_FIELD: [{"value": "Impediment"}]}));
    let http = session(&[json!({})]);
    let off = call(&http, actions::SET_IMPEDIMENT, json!({"issue": "BIG-1", "on": false}), "k6").await;
    assert_eq!(off, Ok(json!({"on": false, "reason": null})));
    assert_eq!(tool_calls(&http)[0].1["fields"], json!({DEFAULT_FLAG_FIELD: null}));
}

#[tokio::test]
async fn update_issue_edits_the_title_and_description_as_markdown() {
    let http = session(&[json!({"key": "BIG-9", "fields": {"summary": "Pacote 2", "status": {"name": "Aberto"}}})]);
    let answer =
        call(&http, actions::UPDATE_ISSUE, json!({"issue": "BIG-9", "title": "Pacote 2", "description": "notas"}), "u1")
            .await;
    assert_eq!(answer.map(|value| value["ref"].clone()), Ok(json!("BIG-9")));
    let (name, arguments) = tool_calls(&http).remove(0);
    assert_eq!(name, "editJiraIssue");
    assert_eq!(arguments["fields"], json!({"summary": "Pacote 2", "description": "notas"}));
    assert_eq!(arguments["contentFormat"], "markdown");
    let empty = call(&session(&[]), actions::UPDATE_ISSUE, json!({"issue": "BIG-9"}), "u2").await;
    assert!(empty.is_err());
}

#[tokio::test]
async fn a_link_already_there_is_not_created_again() {
    let links = json!({"fields": {"issuelinks": [{"inwardIssue": {"key": "APP-1"}}]}});
    let http = session(&[links]);
    let answer = call(&http, actions::LINK_ISSUES, json!({"issue": "BIG-9", "to": "APP-1"}), "l1").await;
    assert_eq!(answer, Ok(json!({"issue": "BIG-9", "to": "APP-1", "already": true})));
    assert_eq!(tool_calls(&http).len(), 1);
    let http = session(&[json!({"fields": {"issuelinks": []}}), json!({})]);
    let answer = call(&http, actions::LINK_ISSUES, json!({"issue": "BIG-9", "to": "APP-2"}), "l2").await;
    assert_eq!(answer, Ok(json!({"issue": "BIG-9", "to": "APP-2", "already": false})));
    let (name, arguments) = tool_calls(&http).remove(1);
    assert_eq!(name, "createIssueLink");
    assert_eq!((arguments["inwardIssue"].clone(), arguments["outwardIssue"].clone()), (json!("APP-2"), json!("BIG-9")));
}

fn status(id: &str, name: &str, category: &str) -> Value {
    json!({"id": id, "name": name, "statusCategory": {"key": category}})
}

fn names(listed: &Result<Value, ActionError>) -> Vec<Value> {
    let found = listed.as_ref().ok().and_then(Value::as_array).cloned().unwrap_or_default();
    found.iter().map(|status| status["name"].clone()).collect()
}

#[tokio::test]
async fn statuses_are_every_status_the_workflows_of_the_project_have_even_unused_ones() {
    let grouped = json!([
        {"name": "Bug", "statuses": [status("1", "A fazer", "new"), status("3", "Feito", "done")]},
        {"name": "Tarefa", "statuses": [status("1", "A fazer", "new"), status("4", "Bloqueado", "indeterminate")]},
    ]);
    let http = session(&[grouped]);
    let listed = call(&http, actions::STATUSES, json!({}), "").await;
    assert_eq!(names(&listed), [json!("A fazer"), json!("Feito"), json!("Bloqueado")]);
    assert_eq!(listed.map(|l| l[2]["category"].clone()), Ok(json!("indeterminate")));
    let calls = tool_calls(&http);
    assert_eq!(calls.len(), 1);
    assert_eq!((calls[0].0.as_str(), &calls[0].1["name"]), ("executeRead", &json!("listJiraStatuses")));
    assert_eq!(calls[0].1["inputs"]["projectIdOrKey"], "BIG");
    let flat = json!({"values": [{"id": "9", "name": "Revisão", "statusCategory": "IN_PROGRESS"}], "isLast": true});
    let listed = call(&session(&[flat]), actions::STATUSES, json!({"project": "OPS"}), "").await;
    assert_eq!(listed, Ok(json!([{"id": "9", "name": "Revisão", "category": "indeterminate"}])));
}

#[tokio::test]
async fn without_a_status_list_the_statuses_come_from_one_issue_of_every_type_and_its_transitions() {
    let issue =
        |key: &str, kind: &str, now: Value| json!({"key": key, "fields": {"status": now, "issuetype": {"name": kind}}});
    let http = session(&[
        json!("nothing matched"),
        json!({"issues": [
            issue("BIG-1", "Bug", status("1", "A fazer", "new")),
            issue("BIG-2", "Bug", status("2", "Fazendo", "indeterminate")),
            issue("BIG-3", "Tarefa", status("1", "A fazer", "new")),
        ]}),
        json!({"transitions": [{"id": "5", "to": status("3", "Feito", "done")}]}),
        json!({"transitions": [{"id": "6", "to": status("8", "Revisão", "indeterminate")}]}),
    ])
    .knowing(&["listJiraIssueTransitions"]);
    let listed = call(&http, actions::STATUSES, json!({}), "").await;
    assert_eq!(names(&listed), [json!("A fazer"), json!("Fazendo"), json!("Feito"), json!("Revisão")]);
    let calls = tool_calls(&http);
    assert_eq!(calls[2].0, "searchJiraIssuesUsingJql");
    assert!(calls[2].1["fields"].as_array().is_some_and(|fields| fields.contains(&json!("issuetype"))));
    assert_eq!(calls[3].1["inputs"]["issueIdOrKey"], "BIG-1");
    assert_eq!(calls[4].1["inputs"]["issueIdOrKey"], "BIG-3");
}

fn issue_types() -> Value {
    json!({"issueTypes": [
        {"id": "10019", "name": "Bug", "untranslatedName": "Bug"},
        {"id": "10020", "name": "Tarefa", "untranslatedName": "Task"},
    ]})
}

fn type_fields() -> Value {
    let field = |key: &str, name: &str, schema: Value| json!({"required": true, "key": key, "fieldId": key, "name": name, "schema": schema, "hasDefaultValue": false});
    let mut reporter = field("reporter", "Relator", json!({"type": "user"}));
    reporter["hasDefaultValue"] = json!(true);
    let mut ambiente = field("customfield_10050", "Ambiente", json!({"type": "option"}));
    ambiente["allowedValues"] = json!([{"id": "1", "value": "Produção"}, {"id": "2", "value": "Homologação"}]);
    let mut components = field("components", "Componentes", json!({"type": "array", "items": "component"}));
    components["allowedValues"] = json!([{"id": "7", "name": "API"}]);
    json!({"fields": [
        field("issuetype", "Tipo de item", json!({"type": "issuetype"})),
        field("project", "Projeto", json!({"type": "project"})),
        reporter,
        field("summary", "Resumo", json!({"type": "string"})),
        ambiente,
        field("customfield_10051", "Cliente", json!({"type": "string"})),
        components,
    ]})
}

#[tokio::test]
async fn fields_are_the_required_fields_of_the_issue_type_that_nothing_else_fills() {
    let http = session(&[issue_types(), type_fields()]);
    let found = call(&http, actions::FIELDS, json!({}), "").await;
    let ambiente = json!({"key": "customfield_10050", "name": "Ambiente", "type": "option", "required": true,
        "values": [{"id": "1", "name": "Produção"}, {"id": "2", "name": "Homologação"}]});
    let cliente = json!({"key": "customfield_10051", "name": "Cliente", "type": "string", "required": true});
    let components = json!({"key": "components", "name": "Componentes", "type": "array", "required": true,
        "values": [{"id": "7", "name": "API"}]});
    assert_eq!(found, Ok(json!([ambiente, cliente, components])));
    let calls = tool_calls(&http);
    assert_eq!(calls[0].1["name"], "listJiraProjectIssueTypesMetadata");
    assert_eq!(calls[0].1["inputs"]["projectIdOrKey"], "BIG");
    assert_eq!(calls[1].1["name"], "getJiraIssueTypeMetaWithFields");
    assert_eq!(
        calls[1].1["inputs"]["issueTypeId"], "10020",
        "no type is the default Task, found by its untranslated name"
    );
    let http = session(&[issue_types()]);
    let unknown = call(&http, actions::FIELDS, json!({"project": "BIG", "type": "Épico"}), "").await;
    assert_eq!(unknown.map_err(|e| e.class), Err(ErrorClass::Permanent));
}

#[tokio::test]
async fn create_issue_sends_the_extra_fields_in_the_shape_each_field_takes() {
    let http = session(&[issue_types(), type_fields(), json!({"id": "11", "key": "BIG-11"})]);
    let fields =
        json!({"customfield_10050": "2", "customfield_10051": "ACME", "components": "7", "customfield_9": "x"});
    let args = json!({"project": "BIG", "title": "T", "type": "bug", "fields": fields});
    let answer = call(&http, actions::CREATE_ISSUE, args, "").await;
    assert_eq!(answer.map(|a| a["ref"].clone()), Ok(json!("BIG-11")));
    let calls = tool_calls(&http);
    assert_eq!(calls[1].1["inputs"]["issueTypeId"], "10019");
    let (name, sent) = calls[2].clone();
    assert_eq!(name, "createJiraIssue");
    assert_eq!(
        sent["additional_fields"],
        json!({"customfield_10050": {"id": "2"}, "customfield_10051": "ACME", "components": [{"id": "7"}], "customfield_9": "x"})
    );
}

#[tokio::test]
async fn an_issue_reads_description_subtasks_comments_and_the_flag() {
    let answer = json!({ "id": 10001, "key": "BIG-1", "fields": {
        "summary": "T", "status": {"name": "Fazendo"}, "labels": [],
        "description": {"type": "doc", "version": 1, "content": [{"type": "paragraph", "content": [{"type": "text", "text": "d"}]}]},
        "subtasks": [{"key": "BIG-2", "fields": {"summary": "s", "status": {"name": "Feito", "statusCategory": {"key": "done"}}}}],
        "issuetype": {"name": "Story"}, "project": {"id": "10003", "key": "BIG"}, "assignee": {"accountId": "u1", "displayName": "Bia"},
        "attachment": [{"filename": "a.png", "size": 3, "content": "https://f"}],
        "comment": {"comments": [{"id": "5", "author": {"displayName": "Ana"}, "body": "texto", "created": "2024-01-01T00:00:00.000+0000"}]},
        DEFAULT_FLAG_FIELD: [{"value": "Impediment"}],
    }});
    let http = session(&[answer]);
    let read = call(&http, actions::ISSUE, json!({"issue": "BIG-1"}), "").await;
    let Ok(read) = read else { panic!("{read:?}") };
    assert_eq!(read["description"], "d");
    assert_eq!(read["ref"], "BIG-1");
    assert_eq!(read["id"], "10001");
    assert_eq!(read["impediment"], json!({"on": true, "reason": null}));
    assert_eq!(
        read["subtasks"][0],
        json!({"ref": "BIG-2", "title": "s", "status": "Feito", "url": "https://acme.atlassian.net/browse/BIG-2", "done": true})
    );
    assert_eq!(read["comments"][0]["body"], "texto");
    assert_eq!(read["comments"][0]["author"], json!({"id": "Ana", "name": "Ana"}));
    assert_eq!(read["type"], "Story");
    assert_eq!(read["project"], "BIG");
    assert_eq!(read["assignee"], json!({"id": "u1", "name": "Bia"}));
    assert_eq!(read["attachments"][0]["name"], "a.png");
    assert_eq!(tool_calls(&http)[0].1["responseContentFormat"], "markdown");
    assert_eq!(tool_calls(&http)[0].1["view"], "full");
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
    assert_eq!(batch.events[0].data["author"], json!({"id": "Ana", "name": "Ana"}));
    assert_eq!(batch.events[1].data["to"], "Feito");
    assert_eq!(batch.events[1].id, "jira:BIG-1:status:h1");
    assert_eq!(batch.cursor, "2024-01-02T10:00:00Z");
    let calls = tool_calls(&http);
    assert_eq!(calls[0].1["jql"], "project = \"BIG\" AND updated >= \"2024-01-01 09:00\" ORDER BY updated ASC");
    assert_eq!(calls[1].1["expand"], "changelog");
}

#[tokio::test]
async fn search_reads_the_query_falls_back_to_the_connection_project_and_pages_by_cursor() {
    let page = json!({ "isLast": false, "nextPageToken": "t2", "issues": [{"key": "BIG-3", "fields": {"summary": "x", "status": {"name": "Feito"}}}] });
    let http = session(&[page]);
    let found = call(&http, actions::SEARCH, json!({"query": "login!", "limit": 5, "cursor": "t1"}), "").await;
    assert_eq!(
        found,
        Ok(
            json!({"issues": [{"ref": "BIG-3", "title": "x", "status": "Feito", "url": "https://acme.atlassian.net/browse/BIG-3"}], "cursor": "t2"})
        )
    );
    let sent = tool_calls(&http)[0].1.clone();
    assert_eq!(sent["jql"], "project = \"BIG\" AND text ~ \"login\" ORDER BY updated DESC");
    assert_eq!(sent["nextPageToken"], "t1");
}

#[test]
fn a_flag_change_in_the_history_is_an_impediment_event_with_booleans() {
    let creds = creds();
    let since = jiff::Timestamp::from_second(0).unwrap_or(jiff::Timestamp::UNIX_EPOCH);
    let mut collector = events::Collector { creds: &creds, since, events: Vec::new() };
    let history = json!({"id": "h2", "created": "2024-01-02T10:00:00.000+0000", "author": {"displayName": "Ana"},
        "items": [{"field": "Flagged", "fromString": "", "toString": "Impediment"}]});
    collector.history("BIG-1", &history);
    assert_eq!(collector.events[0].kind, "tracker.impediment.changed");
    assert_eq!(collector.events[0].data, json!({"from": false, "to": true, "author": "Ana"}));
}

#[tokio::test]
async fn the_site_is_found_however_the_server_wraps_the_list() {
    let one = json!({"values": [{"id": "1", "key": "APP", "name": "App"}], "isLast": true});
    for sites in [real_sites(), json!({"result": real_sites()}), json!({"resources": real_sites()})] {
        let http = structured(&[sites.clone(), one.clone()]);
        let options = Jira.accounts(&http, &creds()).await;
        let Ok(options) = options else { panic!("{sites}: {options:?}") };
        assert_eq!(options.len(), 1, "{sites}");
        assert_eq!(options[0].label, "minhaempresa.atlassian.net");
        assert_eq!(options[0].settings["cloud_id"], "0aeb9d87-1111");
        assert_eq!(options[0].settings["project"], "APP", "the only project becomes the default");
    }
    let http = session(&[json!({"result": real_sites()}), json!({"values": []})]);
    let options = Jira.accounts(&http, &creds()).await;
    assert_eq!(options.map(|o| o[0].settings.get("project").cloned()), Ok(None));
}

#[tokio::test]
async fn the_site_of_the_second_version_of_the_server_comes_as_cloud_id() {
    let v2 = json!([{"cloudId": "0aeb9d87-2222", "products": [{"access": "read-write", "id": "jira"}], "url": "https://minhaempresa.atlassian.net"}]);
    let http = structured(&[v2, json!({"values": [], "isLast": true})]);
    let options = Jira.accounts(&http, &creds()).await;
    let Ok(options) = options else { panic!("{options:?}") };
    assert_eq!(options[0].id, "0aeb9d87-2222");
    assert_eq!(options[0].settings["cloud_id"], "0aeb9d87-2222");
    assert_eq!(options[0].label, "minhaempresa.atlassian.net");
}

#[tokio::test]
async fn an_account_without_sites_fails_the_sign_in() {
    for empty in [json!([]), json!({"result": []}), json!("nothing here")] {
        let http = session(std::slice::from_ref(&empty));
        let refused = Jira.accounts(&http, &creds()).await;
        assert_eq!(refused.map_err(|e| e.message), Err(NO_SITES.to_owned()), "{empty}");
    }
    assert_eq!(Jira.chooses(), ["cloud_id"]);
}

#[tokio::test]
async fn a_connection_without_a_site_or_a_project_is_blocked() {
    let http = session(&[]);
    let bare = Credentials::new(&ConnectionId::from("jira1"), "at", json!({}));
    let refused = Jira.call(&http, &bare, actions::SEARCH, &json!({"query": "x"}), "").await;
    assert_eq!(refused.as_ref().map_err(ActionError::blocker), Err(Some(NO_SITE)));
    let http = session(&[]);
    let siteless = Credentials::new(&ConnectionId::from("jira1"), "at", json!({"cloud_id": "c1"}));
    let refused = Jira.call(&http, &siteless, actions::CREATE_ISSUE, &json!({"title": "T"}), "").await;
    assert_eq!(refused.as_ref().map_err(ActionError::blocker), Err(Some(NO_PROJECT)));
    assert!(tool_calls(&http).is_empty());
}

#[tokio::test]
async fn projects_list_the_visible_projects_of_the_site_and_page_through_them() {
    let full: Vec<Value> = (0..50).map(|n| json!({"id": n.to_string(), "key": format!("P{n}"), "name": "x"})).collect();
    let last = json!({"values": [{"id": "99", "key": "OPS", "name": "Operações", "avatarUrls": {"48x48": "https://a/ops.png"}}], "isLast": true});
    let http = session(&[json!({"values": full}), last]);
    let answer = call(&http, actions::PROJECTS, json!({"query": " op "}), "").await;
    let Ok(Value::Array(projects)) = answer else { panic!("{answer:?}") };
    assert_eq!(projects.len(), 51);
    assert_eq!(projects[50], json!({"id": "99", "key": "OPS", "name": "Operações", "avatar": "https://a/ops.png"}));
    let calls = tool_calls(&http);
    assert_eq!(calls[0].0, "executeRead");
    assert_eq!(calls[0].1["name"], "listJiraProjects");
    assert_eq!(calls[0].1["cloudId"], "cloud1");
    assert_eq!(calls[0].1["inputs"]["query"], "op");
    assert_eq!(calls[1].1["inputs"]["startAt"], 50);
}

#[tokio::test]
async fn a_created_issue_goes_to_the_chosen_project_and_reads_a_wrapped_answer() {
    let http = structured(&[json!({"issues": []}), json!({"result": {"id": "10", "key": "OPS-3"}})]);
    let answer = call(&http, actions::CREATE_ISSUE, json!({"project": "OPS", "title": "T"}), "k").await;
    assert_eq!(answer.map(|a| (a["ref"].clone(), a["id"].clone())), Ok((json!("OPS-3"), json!("10"))));
    assert_eq!(tool_calls(&http)[1].1["projectKey"], "OPS");
}
#[tokio::test]
async fn the_fake_server_refuses_the_tools_only_the_first_version_had() {
    let http = session(&[json!({})]);
    let mcp = McpClient::connect(&http, brand::connectors::ATLASSIAN_MCP_URL, "at").await;
    let Ok(mcp) = mcp else { panic!("{mcp:?}") };
    for gone in ["getVisibleJiraProjects", "getTransitionsForJiraIssue", "addCommentToJiraIssue"] {
        let refused = mcp.call(gone, json!({"cloudId": "c1"})).await;
        assert!(refused.as_ref().is_err_and(|e| e.message.contains("Unknown tool")), "{gone}: {refused:?}");
    }
    let refused = mcp.call("executeRead", json!({"cloudId": "c1", "name": "getVisibleJiraProjects"})).await;
    assert!(refused.is_err());
    let siteless = mcp.call("getJiraIssue", json!({"issueIdOrKey": "BIG-1"})).await;
    assert!(siteless.is_err());
}

#[tokio::test]
async fn projects_are_found_through_discover_when_the_operation_has_another_name() {
    let discovered = json!(
        "Matches:\n1. getJiraProject (executeRead): get one Jira project\n   inputs: projectIdOrKey\n\
        2. searchJiraProjects (executeRead): search the Jira projects visible to the user\n   inputs: query, startAt, maxResults"
    );
    let page = json!({"values": [{"id": "1", "key": "APP", "name": "App"}], "isLast": true});
    let http = session(&[discovered, page]).knowing(&["listJiraIssueTransitions", "searchJiraProjects"]);
    let answer = call(&http, actions::PROJECTS, json!({}), "").await;
    let Ok(Value::Array(projects)) = answer else { panic!("{answer:?}") };
    assert_eq!(projects[0]["key"], "APP");
    let calls = tool_calls(&http);
    let names: Vec<(&str, &str)> =
        calls.iter().map(|(tool, sent)| (tool.as_str(), sent["name"].as_str().unwrap_or_default())).collect();
    assert_eq!(names, [("executeRead", "listJiraProjects"), ("discover", ""), ("executeRead", "searchJiraProjects")]);
    assert_eq!(calls[1].1["query"], "list jira projects visible to the user");
}

#[tokio::test]
async fn transitions_are_found_through_discover_and_an_unknown_answer_keeps_the_first_error() {
    let discovered = json!({"operations": [
        {"name": "getJiraIssue", "summary": "Get an issue"},
        {"name": "getJiraIssueTransitions", "tool": "executeRead", "summary": "Transitions available for an issue"}
    ]});
    let http = session(&[
        json!({"fields": {"status": {"name": "A fazer"}}}),
        discovered,
        json!({"result": {"transitions": [{"id": "5", "name": "Concluir", "to": {"name": "Feito"}}]}}),
        json!({"key": "BIG-1"}),
    ])
    .knowing(&["listJiraProjects", "getJiraIssueTransitions"]);
    let answer = call(&http, actions::TRANSITION, json!({"issue": "BIG-1", "status": "Concluir"}), "k").await;
    assert_eq!(answer, Ok(json!({"ref": "BIG-1", "status": "Feito", "changed": true})));
    let calls = tool_calls(&http);
    assert_eq!(calls[3].1["name"], "getJiraIssueTransitions");
    assert_eq!(calls[4].1["transitionId"], "5");
    let http = session(&[json!({"fields": {"status": {"name": "A fazer"}}}), json!("nothing matched")]).knowing(&[]);
    let answer = call(&http, actions::TRANSITION, json!({"issue": "BIG-1", "status": "Feito"}), "k").await;
    assert!(answer.is_err_and(|e| e.message.contains("Unknown operation")));
}

#[test]
fn an_operation_is_picked_from_what_discover_lists() {
    let text =
        json!("- getJiraProject: one project\n- listJiraProjects: projects visible to you\n  inputs: issueIdOrKey");
    assert_eq!(operations::discovered(&text, "project", "x"), Some("listJiraProjects".to_owned()));
    assert_eq!(operations::discovered(&text, "project", "listJiraProjects"), Some("getJiraProject".to_owned()));
    assert_eq!(operations::discovered(&json!("transitionId is required"), "transition", "x"), None);
    let nested = json!({"data": {"projects": [{"key": "A"}], "isLast": true}});
    assert_eq!(operations::found_list(&nested, &["values", "projects"]), vec![json!({"key": "A"})]);
    assert!(operations::last_page(&nested));
}

#[tokio::test]
async fn an_api_token_reaches_the_mcp_server_as_basic_auth() {
    let [alternative] =
        Jira.alternatives(domain::Language::Portuguese).try_into().unwrap_or_else(|_| panic!("not one alternative"));
    let AuthSpec::Token(spec) = alternative.auth else { panic!("not a token") };
    let token = spec
        .access_token(&json!({ "email": "ana@acme.com", "token": "t1" }), domain::Language::Portuguese)
        .unwrap_or_default();
    let http =
        session(&[json!([{"id": "c1", "url": "https://acme.atlassian.net", "name": "acme"}]), json!({"values": []})]);
    let creds = Credentials::new(&ConnectionId::from("jira2"), &token, json!({}));
    let options = Jira.accounts(&http, &creds).await;
    assert!(options.is_ok(), "{options:?}");
    assert_eq!(http.sent()[0].header_value("authorization"), Some("Basic YW5hQGFjbWUuY29tOnQx"));
}
