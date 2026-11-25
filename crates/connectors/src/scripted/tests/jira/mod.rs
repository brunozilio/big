mod adf;
mod attach;
mod fake;
mod pure;

use std::sync::Arc;

use domain::Language;
use serde_json::{Value, json};

use super::{creds as creds_with, loaded, now};
use crate::descriptor::AuthSpec;
use crate::error::{ActionError, ErrorClass};
use crate::http::{FakeHttp, Http, HttpResponse, Method};
use crate::mcp::McpClient;
use crate::provider::{Credentials, Provider};
use crate::scripted::bridge::{WRAPPER, wrapper};
use crate::scripted::{Local, Script, ScriptedProvider};
use fake::AtlassianV2;

const MCP: &str = "/v2/mcp";
const SERVER: &str = "https://mcp.atlassian.com/v2/mcp";
const NO_SITE: &str = "Reconnect Jira and choose the site.";
const NO_PROJECT: &str = "Choose the project.";
const NO_SITES: &str =
    "No Jira site was found in this Atlassian account. Check that the account has access to Jira and connect again.";
const DEFAULT_FLAG_FIELD: &str = "customfield_10021";

fn jira() -> ScriptedProvider {
    loaded("jira")
}

pub fn probe(module: &str, body: &str, input: Value) -> Value {
    let mut modules = content::modules_in(None, module).unwrap_or_else(|error| panic!("{error}"));
    modules.push((
        "@probe".to_owned(),
        format!("import * as m from {module:?};\nexport function probe(input, ctx) {{ {body} }}\n"),
    ));
    modules.push((WRAPPER.to_owned(), wrapper("@probe")));
    let script = Script { modules: Arc::new(modules), data: json!({ "brand": { "COMMAND": brand::COMMAND } }) };
    script.invoke_here("probe", &input, Arc::new(Local)).unwrap_or_else(|error| panic!("{error:?}"))
}

fn label(key: &str) -> String {
    let found = probe("sdk/connectors/index.ts", "return m.idempotencyLabel(ctx, input);", json!(key));
    found.as_str().unwrap_or_default().to_owned()
}

fn creds() -> Credentials {
    creds_with("at", json!({ "cloud_id": "cloud1", "site_url": "https://acme.atlassian.net/", "project": "BIG" }))
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
    let provider = jira();
    let answer = provider.call(http, &creds(), action, &args, key).await;
    if let Ok(value) = &answer {
        let spec = provider.descriptor().action(action).cloned().unwrap_or_else(|| panic!("no {action}"));
        assert_eq!(crate::descriptor::conforms(&spec.output_schema, value), Ok(()), "{action}: {value}");
    }
    answer
}

#[tokio::test]
async fn the_sign_in_is_the_configured_mcp_server_with_dynamic_registration() {
    let AuthSpec::Loopback(sign_in) = jira().descriptor().auth else { panic!("not loopback") };
    assert_eq!(sign_in.0.name(), SERVER);
    assert_eq!(sign_in.0.callback_port(), None);
    let resource =
        r#"{"resource":"https://mcp.atlassian.com/v2/mcp","authorization_servers":["https://auth.atlassian.com"]}"#;
    let server = r#"{"authorization_endpoint":"https://auth.atlassian.com/authorize","token_endpoint":"https://auth.atlassian.com/token","registration_endpoint":"https://auth.atlassian.com/register"}"#;
    let http = FakeHttp::new()
        .once(Method::Get, "/.well-known/oauth-protected-resource", 200, resource)
        .once(Method::Get, "/.well-known/oauth-authorization-server", 200, server)
        .once(Method::Post, "/register", 201, r#"{"client_id":"dyn1"}"#);
    let started = sign_in.0.begin(&http, 4321).await;
    let Ok(started) = started else { panic!("{started:?}") };
    assert!(started.url.contains("resource=https%3A%2F%2Fmcp.atlassian.com%2Fv2%2Fmcp"), "{}", started.url);
    assert!(started.url.contains("offline_access"), "{}", started.url);
    assert!(started.redirect_uri.contains("4321"), "{}", started.redirect_uri);
    assert_eq!(jira().descriptor().name, "Jira");
    assert_eq!(jira().presentation(Language::Portuguese).description, "Sua conta Atlassian");
    assert_eq!(jira().presentation(Language::English).description, "Your Atlassian account");
}

#[tokio::test]
async fn the_sites_come_from_the_accessible_resources_with_their_projects() {
    let http = session(&[
        json!([{"id": "c1", "url": "https://acme.atlassian.net", "name": "acme"}]),
        json!({"values": [{"key": "APP"}, {"key": "OPS"}]}),
    ]);
    let options = jira().accounts(&http, &creds()).await;
    let Ok(options) = options else { panic!("{options:?}") };
    assert_eq!(options[0].id, "c1");
    assert_eq!(options[0].label, "acme.atlassian.net");
    assert_eq!(options[0].settings["cloud_id"], "c1");
    let detail = crate::texts::detail(Language::Portuguese, &options[0].settings);
    assert_eq!(detail.as_deref(), Some("Projetos APP, OPS"));
    let detail = crate::texts::detail(Language::English, &options[0].settings);
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
    let answer = call(&http, "tracker.create_issue", json!({"title": "Fix it!"}), "k1").await;
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
    let jql = format!("project = \"BIG\" AND labels = \"{}\" ORDER BY created DESC", crate::idempotency::label("k1"));
    assert_eq!(calls[0].1["jql"], jql);
}

#[tokio::test]
async fn create_issue_creates_with_the_key_as_a_label() {
    let types = json!({"issueTypes": [
        {"id": "1", "name": "Épico", "hierarchyLevel": 1},
        {"id": "2", "name": "História", "hierarchyLevel": 0},
        {"id": "3", "name": "Construção", "hierarchyLevel": -1, "subtask": true}
    ]});
    let parent = json!({"key": "BIG-1", "fields": {"issuetype": {"name": "História"}}});
    let http =
        session(&[json!({ "issues": [] }), types, json!([]), parent, json!({"id": "10", "key": "BIG-10"})]);
    let args = json!({"project": "BIG", "title": "T", "description": "**b**", "parent": "BIG-1", "labels": ["x"]});
    let answer = call(&http, "tracker.create_issue", args, "k2").await;
    assert_eq!(
        answer.map(|a| (a["ref"].clone(), a["title"].clone(), a["created"].clone())),
        Ok((json!("BIG-10"), json!("T"), json!(true)))
    );
    let (name, sent) = tool_calls(&http)[4].clone();
    assert_eq!(name, "createJiraIssue");
    assert_eq!(sent["labels"], json!(["x", label("k2")]));
    assert_eq!(label("k2"), crate::idempotency::label("k2"));
    assert_eq!(sent["issueType"], "Construção");
    assert_eq!(sent["parent"], "BIG-1");
    assert_eq!(sent["contentFormat"], "markdown");
    assert_eq!(sent["description"], "**b**");
}

#[tokio::test]
async fn transition_to_the_current_status_is_already_done() {
    let http = session(&[json!({"key": "BIG-1", "fields": {"status": {"name": "Fazendo"}}})]);
    let answer = call(&http, "tracker.transition", json!({"issue": "BIG-1", "status": "fazendo"}), "k").await;
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
    let answer = call(&http, "tracker.transition", json!({"issue": "BIG-1", "status": "Feito"}), "k").await;
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
    let answer = call(&http, "tracker.transition", json!({"issue": "BIG-1", "status": "Feito"}), "k").await;
    assert_eq!(answer.map_err(|e| e.class), Err(ErrorClass::Permanent));
}

#[tokio::test]
async fn a_comment_already_posted_is_not_posted_again() {
    let known = json!({"id": "7", "body": "Oi!", "author": {"accountId": "a1", "displayName": "Ana"}, "created": "2024-01-01T00:00:00.000+0000"});
    let http = session(&[json!({"comments": [known], "isLast": true, "total": 1})]);
    let answer = call(&http, "tracker.comment", json!({"issue": "BIG-1", "body": "oi"}), "k3").await;
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
    let answer = call(&http, "tracker.comment", json!({"issue": "BIG-1", "body": "oi"}), "k4").await;
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
    let args = json!({"issue": "BIG-1", "on": true, "reason": "falta acesso"});
    let on = call(&http, "tracker.set_impediment", args, "k5").await;
    assert_eq!(on, Ok(json!({"on": true, "reason": "falta acesso"})));
    let names: Vec<String> = tool_calls(&http).into_iter().map(|(name, _)| name).collect();
    assert_eq!(names, ["editJiraIssue", "listJiraIssueComments", "addOrEditJiraIssueComment"]);
    assert_eq!(tool_calls(&http)[0].1["fields"], json!({DEFAULT_FLAG_FIELD: [{"value": "Impediment"}]}));
    let http = session(&[json!({})]);
    let off = call(&http, "tracker.set_impediment", json!({"issue": "BIG-1", "on": false}), "k6").await;
    assert_eq!(off, Ok(json!({"on": false, "reason": null})));
    assert_eq!(tool_calls(&http)[0].1["fields"], json!({DEFAULT_FLAG_FIELD: null}));
}

#[tokio::test]
async fn update_issue_edits_the_title_and_description_as_markdown() {
    let http = session(&[json!({"key": "BIG-9", "fields": {"summary": "Pacote 2", "status": {"name": "Aberto"}}})]);
    let args = json!({"issue": "BIG-9", "title": "Pacote 2", "description": "notas"});
    let answer = call(&http, "tracker.update_issue", args, "u1").await;
    assert_eq!(answer.map(|value| value["ref"].clone()), Ok(json!("BIG-9")));
    let (name, arguments) = tool_calls(&http).remove(0);
    assert_eq!(name, "editJiraIssue");
    assert_eq!(arguments["fields"], json!({"summary": "Pacote 2", "description": "notas"}));
    assert_eq!(arguments["contentFormat"], "markdown");
    let empty = call(&session(&[]), "tracker.update_issue", json!({"issue": "BIG-9"}), "u2").await;
    assert!(empty.is_err());
}

#[tokio::test]
async fn a_link_already_there_is_not_created_again() {
    let links = json!({"fields": {"issuelinks": [{"inwardIssue": {"key": "APP-1"}}]}});
    let http = session(&[links]);
    let answer = call(&http, "tracker.link_issues", json!({"issue": "BIG-9", "to": "APP-1"}), "l1").await;
    assert_eq!(answer, Ok(json!({"issue": "BIG-9", "to": "APP-1", "already": true})));
    assert_eq!(tool_calls(&http).len(), 1);
    let http = session(&[json!({"fields": {"issuelinks": []}}), json!({})]);
    let answer = call(&http, "tracker.link_issues", json!({"issue": "BIG-9", "to": "APP-2"}), "l2").await;
    assert_eq!(answer, Ok(json!({"issue": "BIG-9", "to": "APP-2", "already": false})));
    let (name, arguments) = tool_calls(&http).remove(1);
    assert_eq!(name, "createIssueLink");
    assert_eq!((arguments["inwardIssue"].clone(), arguments["outwardIssue"].clone()), (json!("APP-2"), json!("BIG-9")));
}

mod metadata;
mod sites;

#[tokio::test]
async fn a_rejected_token_is_an_auth_error_and_a_bad_key_never_reaches_a_tool() {
    let http = FakeHttp::new().once(Method::Post, MCP, 401, r#"{"error":"invalid_token"}"#);
    let refused = call(&http, "tracker.issue", json!({"issue": "BIG-1"}), "").await;
    assert_eq!(refused.map_err(|e| e.class), Err(ErrorClass::Auth));
    let http = session(&[]);
    let injected = call(&http, "tracker.issue", json!({"issue": "BIG-1\" OR 1=1"}), "").await;
    assert_eq!(injected.map_err(|e| e.class), Err(ErrorClass::Permanent));
    assert!(tool_calls(&http).is_empty());
    let _ = HttpResponse::new(200, "");
}

#[tokio::test]
async fn the_fake_server_refuses_the_tools_only_the_first_version_had() {
    let http = session(&[json!({})]);
    let mcp = McpClient::connect(&http, SERVER, "at").await;
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
async fn an_api_token_reaches_the_mcp_server_as_basic_auth() {
    let [alternative] =
        jira().alternatives(Language::Portuguese).try_into().unwrap_or_else(|_| panic!("not one alternative"));
    assert_eq!(alternative.key, "api_token");
    assert_eq!(alternative.label, "Usar API token");
    let AuthSpec::Token(spec) = alternative.auth else { panic!("not a token") };
    let token =
        spec.access_token(&json!({ "email": "ana@acme.com", "token": "t1" }), Language::Portuguese).unwrap_or_default();
    let http =
        session(&[json!([{"id": "c1", "url": "https://acme.atlassian.net", "name": "acme"}]), json!({"values": []})]);
    let options = jira().accounts(&http, &creds_with(&token, json!({}))).await;
    assert!(options.is_ok(), "{options:?}");
    assert_eq!(http.sent()[0].header_value("authorization"), Some("Basic YW5hQGFjbWUuY29tOnQx"));
    let _ = now();
}
