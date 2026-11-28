use domain::Language;
use serde_json::{Value, json};

use super::{NO_PROJECT, NO_SITE, NO_SITES, call, creds, jira, real_sites, session, structured, tool_calls};
use crate::error::ActionError;
use crate::http::FakeHttp;
use crate::provider::Provider;
use crate::scripted::tests::creds as creds_with;

#[tokio::test]
async fn the_first_poll_starts_the_cursor_and_later_polls_read_changes_since_it() {
    let provider = jira();
    let source = provider.events().unwrap_or_else(|| panic!("no events"));
    let http = FakeHttp::new();
    let first = source.poll(&http, &creds(), None, &[]).await;
    assert!(first.as_ref().is_ok_and(|b| b.events.is_empty() && !b.cursor.is_empty()), "{first:?}");
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
    let batch = source.poll(&http, &creds(), Some("2024-01-02T00:00:00Z"), &[]).await;
    let Ok(batch) = batch else { panic!("{batch:?}") };
    let kinds: Vec<&str> = batch.events.iter().map(|e| e.kind.as_str()).collect();
    assert_eq!(kinds, ["tracker.comment.added", "tracker.status.changed"]);
    assert_eq!(batch.events[0].data["body"], "novo");
    assert_eq!(batch.events[0].data["author"], json!({"id": "Ana", "name": "Ana"}));
    assert_eq!(batch.events[0].connection, creds().connection);
    assert_eq!(batch.events[1].data["to"], "Feito");
    assert_eq!(batch.events[1].id, "jira:BIG-1:status:h1");
    assert_eq!(batch.events[1].at.to_string(), "2024-01-02T10:00:00Z");
    assert_eq!(batch.cursor, "2024-01-02T10:00:00Z");
    let calls = tool_calls(&http);
    assert_window(&calls[0].1["jql"], "project in (\"BIG\")", "2024-01-02T00:00:00Z");
    assert_eq!(calls[1].1["expand"], "changelog");
    assert!(batch.events.iter().all(|event| event.scope == "BIG"));
}

#[tokio::test]
async fn the_poll_watches_every_space_bound_to_the_connection_and_marks_each_event_with_its_space() {
    let page =
        json!({ "isLast": true, "issues": [{"key": "OPS-7", "fields": {"updated": "2024-01-02T10:00:00.000+0000"}}]});
    let detail = json!({"key": "OPS-7", "fields": {"created": "2024-01-02T10:00:00.000+0000", "summary": "S",
        "issuetype": {"name": "Bug"}, "parent": {"key": "OPS-1"}}});
    let provider = jira();
    let source = provider.events().unwrap_or_else(|| panic!("no events"));
    let http = session(&[page, detail]);
    let watch = ["WEB".to_owned(), "OPS".to_owned(), "WEB".to_owned()];
    let batch = source.poll(&http, &creds(), Some("2024-01-02T00:00:00Z"), &watch).await;
    let Ok(batch) = batch else { panic!("{batch:?}") };
    assert_eq!(batch.events[0].scope, "OPS");
    assert_eq!(batch.events[0].data["type"], "Bug", "a workflow can be triggered by the issue type");
    assert_eq!(batch.events[0].data["parent"], "OPS-1");
    let calls = tool_calls(&http);
    assert_window(&calls[0].1["jql"], "project in (\"WEB\", \"OPS\")", "2024-01-02T00:00:00Z");
}

fn assert_window(jql: &Value, projects: &str, since: &str) {
    let jql = jql.as_str().unwrap_or_default();
    let minutes = jql
        .strip_prefix(&format!("{projects} AND updated >= \"-"))
        .and_then(|rest| rest.strip_suffix("m\" ORDER BY updated ASC"))
        .and_then(|minutes| minutes.parse::<i64>().ok());
    let since: jiff::Timestamp = since.parse().unwrap_or_else(|error| panic!("{error}"));
    let elapsed = jiff::Timestamp::now().as_second().saturating_sub(since.as_second()) / 60;
    assert!(
        minutes.is_some_and(|minutes| minutes > elapsed && minutes <= elapsed + 7),
        "the window is relative to now, covers the cursor and keeps only a few minutes of margin: {jql}"
    );
}

#[tokio::test]
async fn an_issue_not_updated_since_the_cursor_is_not_read_again() {
    let page = json!({ "isLast": true, "issues": [
        {"key": "BIG-1", "fields": {"updated": "2024-01-01T23:00:00.000+0000"}},
        {"key": "BIG-2", "fields": {"updated": "2024-01-02T00:00:00.000+0000"}}
    ]});
    let provider = jira();
    let source = provider.events().unwrap_or_else(|| panic!("no events"));
    let http = session(&[page]);
    let batch = source.poll(&http, &creds(), Some("2024-01-02T00:00:00Z"), &[]).await;
    let Ok(batch) = batch else { panic!("{batch:?}") };
    assert!(batch.events.is_empty());
    assert_eq!(batch.cursor, "2024-01-02T00:00:00Z");
    let calls = tool_calls(&http);
    assert_eq!(calls.len(), 1, "only the search runs: {calls:?}");
}

#[tokio::test]
async fn a_flag_change_in_the_history_is_an_impediment_event_with_booleans() {
    let page =
        json!({ "isLast": true, "issues": [{"key": "BIG-1", "fields": {"updated": "2024-01-02T10:00:00.000+0000"}}]});
    let detail = json!({"key": "BIG-1", "fields": {"created": "2020-01-01T00:00:00.000+0000"},
        "changelog": {"histories": [{"id": "h2", "created": "2024-01-02T10:00:00.000+0000", "author": {"displayName": "Ana"},
            "items": [{"field": "Flagged", "fromString": "", "toString": "Impediment"}]}]}});
    let provider = jira();
    let source = provider.events().unwrap_or_else(|| panic!("no events"));
    let batch = source.poll(&session(&[page, detail]), &creds(), Some("2024-01-01T00:00:00Z"), &[]).await;
    let Ok(batch) = batch else { panic!("{batch:?}") };
    assert_eq!(batch.events[0].kind, "tracker.impediment.changed");
    assert_eq!(batch.events[0].id, "jira:BIG-1:Flagged:h2");
    assert_eq!(batch.events[0].data, json!({"from": false, "to": true, "author": "Ana"}));
}

#[tokio::test]
async fn search_reads_the_query_falls_back_to_the_connection_project_and_pages_by_cursor() {
    let page = json!({ "isLast": false, "nextPageToken": "t2", "issues": [{"key": "BIG-3", "fields": {"summary": "x", "status": {"name": "Feito"}}}] });
    let http = session(&[page]);
    let found = call(&http, "tracker.search", json!({"query": "login!", "limit": 5, "cursor": "t1"}), "").await;
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

#[tokio::test]
async fn the_site_is_found_however_the_server_wraps_the_list() {
    let one = json!({"values": [{"id": "1", "key": "APP", "name": "App"}], "isLast": true});
    for sites in [real_sites(), json!({"result": real_sites()}), json!({"resources": real_sites()})] {
        let http = structured(&[sites.clone(), one.clone()]);
        let options = jira().accounts(&http, &creds()).await;
        let Ok(options) = options else { panic!("{sites}: {options:?}") };
        assert_eq!(options.len(), 1, "{sites}");
        assert_eq!(options[0].label, "minhaempresa.atlassian.net");
        assert_eq!(options[0].settings["cloud_id"], "0aeb9d87-1111");
        assert_eq!(options[0].settings["project"], "APP", "the only project becomes the default");
    }
    let http = session(&[json!({"result": real_sites()}), json!({"values": []})]);
    let options = jira().accounts(&http, &creds()).await;
    assert_eq!(options.map(|o| o[0].settings.get("project").cloned()), Ok(None));
}

#[tokio::test]
async fn the_site_of_the_second_version_of_the_server_comes_as_cloud_id() {
    let v2 = json!([{"cloudId": "0aeb9d87-2222", "products": [{"access": "read-write", "id": "jira"}], "url": "https://minhaempresa.atlassian.net"}]);
    let http = structured(&[v2, json!({"values": [], "isLast": true})]);
    let options = jira().accounts(&http, &creds()).await;
    let Ok(options) = options else { panic!("{options:?}") };
    assert_eq!(options[0].id, "0aeb9d87-2222");
    assert_eq!(options[0].settings["cloud_id"], "0aeb9d87-2222");
    assert_eq!(options[0].label, "minhaempresa.atlassian.net");
}

#[tokio::test]
async fn an_account_without_sites_fails_the_sign_in() {
    for empty in [json!([]), json!({"result": []}), json!("nothing here")] {
        let http = session(std::slice::from_ref(&empty));
        let refused = jira().accounts(&http, &creds()).await;
        assert_eq!(refused.map_err(|e| e.message), Err(NO_SITES.to_owned()), "{empty}");
    }
    assert_eq!(jira().chooses(), ["cloud_id"]);
}

#[tokio::test]
async fn a_connection_without_a_site_or_a_project_is_blocked() {
    let http = session(&[]);
    let refused = jira().call(&http, &creds_with("at", json!({})), "tracker.search", &json!({"query": "x"}), "").await;
    assert_eq!(refused.as_ref().map_err(ActionError::blocker), Err(Some(NO_SITE)));
    let http = session(&[]);
    let siteless = creds_with("at", json!({"cloud_id": "c1"}));
    let refused = jira().call(&http, &siteless, "tracker.create_issue", &json!({"title": "T"}), "").await;
    assert_eq!(refused.as_ref().map_err(ActionError::blocker), Err(Some(NO_PROJECT)));
    assert!(tool_calls(&http).is_empty());
}

#[test]
fn the_connector_tells_its_blockers_in_the_chosen_language() {
    let told = jira().messages(Language::Portuguese);
    assert_eq!(told.get(NO_SITE).map(String::as_str), Some("Reconecte o Jira e escolha o site."));
    assert_eq!(told.get(NO_PROJECT).map(String::as_str), Some("Escolha o projeto."));
    let spanish = jira().messages(Language::Spanish);
    assert!(spanish.get(NO_SITES).is_some_and(|said| said.starts_with("No se encontró")));
    assert_eq!(jira().messages(Language::English).get(NO_SITE).map(String::as_str), Some(NO_SITE));
}

#[test]
fn the_connector_declares_the_shape_of_its_own_tokens() {
    let declared = jira().secret_patterns();
    assert!(declared.prefixes.iter().any(|rule| rule.prefix == "ATATT"), "{declared:?}");
}

#[tokio::test]
async fn projects_list_the_visible_projects_of_the_site_and_page_through_them() {
    let full: Vec<Value> = (0..50).map(|n| json!({"id": n.to_string(), "key": format!("P{n}"), "name": "x"})).collect();
    let last = json!({"values": [{"id": "99", "key": "OPS", "name": "Operações", "avatarUrls": {"48x48": "https://a/ops.png"}}], "isLast": true});
    let http = session(&[json!({"values": full}), last]);
    let answer = call(&http, "tracker.projects", json!({"query": " op "}), "").await;
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
    let types = json!({"issueTypes": [{"id": "1", "name": "História", "hierarchyLevel": 0}]});
    let http = structured(&[json!({"issues": []}), types, json!([]), json!({"result": {"id": "10", "key": "OPS-3"}})]);
    let answer = call(&http, "tracker.create_issue", json!({"project": "OPS", "title": "T"}), "k").await;
    assert_eq!(answer.map(|a| (a["ref"].clone(), a["id"].clone())), Ok((json!("OPS-3"), json!("10"))));
    assert_eq!(tool_calls(&http)[3].1["projectKey"], "OPS");
    assert_eq!(tool_calls(&http)[3].1["issueType"], "História");
}

#[tokio::test]
async fn projects_are_found_through_discover_when_the_operation_has_another_name() {
    let discovered = json!(
        "Matches:\n1. getJiraProject (executeRead): get one Jira project\n   inputs: projectIdOrKey\n\
        2. searchJiraProjects (executeRead): search the Jira projects visible to the user\n   inputs: query, startAt, maxResults"
    );
    let page = json!({"values": [{"id": "1", "key": "APP", "name": "App"}], "isLast": true});
    let http = session(&[discovered, page]).knowing(&["listJiraIssueTransitions", "searchJiraProjects"]);
    let answer = call(&http, "tracker.projects", json!({}), "").await;
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
    let answer = call(&http, "tracker.transition", json!({"issue": "BIG-1", "status": "Concluir"}), "k").await;
    assert_eq!(answer, Ok(json!({"ref": "BIG-1", "status": "Feito", "changed": true})));
    let calls = tool_calls(&http);
    assert_eq!(calls[3].1["name"], "getJiraIssueTransitions");
    assert_eq!(calls[4].1["transitionId"], "5");
    let http = session(&[json!({"fields": {"status": {"name": "A fazer"}}}), json!("nothing matched")]).knowing(&[]);
    let answer = call(&http, "tracker.transition", json!({"issue": "BIG-1", "status": "Feito"}), "k").await;
    assert!(answer.is_err_and(|e| e.message.contains("Unknown operation")));
}
