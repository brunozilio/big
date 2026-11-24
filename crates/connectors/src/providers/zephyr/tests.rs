use domain::connection::ConnectionId;
use serde_json::json;

use super::fake::ZephyrMcp;
use super::*;
use crate::error::ErrorClass;
use crate::idempotency;

fn creds(settings: Value) -> Credentials {
    Credentials::new(&ConnectionId::from("zephyr1"), "zt", settings)
}

async fn call(http: &dyn Http, action: &str, args: Value) -> Result<Value, ActionError> {
    let answer = Zephyr.call(http, &creds(json!({ "project": "BIG" })), action, &args, "k1").await;
    if let Ok(value) = &answer {
        let spec = Zephyr.descriptor().action(action).cloned().unwrap_or_else(|| panic!("no {action}"));
        assert_eq!(crate::descriptor::conforms(&spec.output_schema, value), Ok(()), "{action}: {value}");
    }
    answer
}

#[test]
fn the_sign_in_is_the_configured_mcp_server_asking_for_the_api_scope() {
    let AuthSpec::Loopback(_) = Zephyr.descriptor().auth else { panic!("not loopback") };
    let flow = sign_in();
    assert_eq!(flow.server, brand::connectors::ZEPHYR_MCP_URL);
    assert_eq!(flow.redirect, brand::connectors::ZEPHYR_OAUTH_REDIRECT);
    assert_eq!(flow.scopes, vec!["api".to_owned()]);
    assert_eq!(Zephyr.presentation().group, "Testes");
    assert!(Zephyr.events().is_none());
}

#[tokio::test]
async fn a_new_case_is_created_given_its_steps_and_linked_to_the_issue() {
    let http = ZephyrMcp::new()
        .answer("zephyr_get_issue_link_test_cases", json!({ "result": [] }))
        .answer("zephyr_get_test_cases", json!({ "values": [], "nextStartAtId": null, "limit": 50 }))
        .answer("zephyr_create_test_case", json!({ "id": 7, "key": "BIG-T7", "self": "https://api/7" }))
        .answer("zephyr_create_test_case_steps", json!({ "id": 1 }))
        .answer("zephyr_create_test_case_issue_link", json!({ "id": 2 }));
    let args = json!({
        "title": "Login works", "objective": "o", "labels": ["auto"], "issue": "BIG-1", "issue_id": "10001",
        "steps": [{ "action": "open", "data": "user", "expected": "logged in" }, { "action": "leave", "expected": "out" }],
    });
    let answer = call(&http, actions::CREATE_CASE, args).await;
    assert_eq!(answer, Ok(json!({ "ref": "BIG-T7", "title": "Login works", "url": "", "created": true })));
    let calls = http.calls();
    let names: Vec<&str> = calls.iter().map(|(name, _)| name.as_str()).collect();
    assert_eq!(
        names,
        [
            "zephyr_get_issue_link_test_cases",
            "zephyr_get_test_cases",
            "zephyr_create_test_case",
            "zephyr_create_test_case_steps",
            "zephyr_create_test_case_issue_link"
        ]
    );
    assert_eq!(calls[0].1, json!({ "issueKey": "BIG-1" }));
    assert_eq!(calls[1].1, json!({ "projectKey": "BIG", "nameOrKey": "Login works", "limit": 50 }));
    assert_eq!(
        calls[2].1,
        json!({
            "projectKey": "BIG", "name": "Login works", "objective": "o",
            "labels": ["auto", idempotency::label("k1")],
        })
    );
    assert_eq!(calls[3].1["mode"], "OVERWRITE");
    assert_eq!(
        calls[3].1["items"],
        json!([
            { "inline": { "description": "open", "testData": "user", "expectedResult": "logged in" } },
            { "inline": { "description": "leave", "expectedResult": "out" } }
        ])
    );
    assert_eq!(calls[4].1, json!({ "testCaseKey": "BIG-T7", "issueId": 10001 }));
}

#[tokio::test]
async fn a_case_already_linked_with_the_same_title_is_reused() {
    let http = ZephyrMcp::new()
        .answer("zephyr_get_issue_link_test_cases", json!({ "result": [{ "key": "BIG-T1" }, { "key": "BIG-T2" }] }))
        .answer("zephyr_get_test_case", json!({ "key": "BIG-T1", "name": "Other" }))
        .answer("zephyr_get_test_case", json!({ "key": "BIG-T2", "name": "login  WORKS" }));
    let args = json!({ "title": "Login works", "issue": "BIG-1", "issue_id": 10001, "steps": [{ "action": "a" }] });
    let answer = call(&http, actions::CREATE_CASE, args).await;
    assert_eq!(answer, Ok(json!({ "ref": "BIG-T2", "title": "login  WORKS", "url": "", "created": false })));
    assert!(http.calls().iter().all(|(name, _)| !name.starts_with("zephyr_create")));
}

#[tokio::test]
async fn the_cases_of_an_issue_carry_their_titles() {
    let http = ZephyrMcp::new()
        .answer("zephyr_get_issue_link_test_cases", json!({ "result": [{ "key": "BIG-T1", "version": 1 }] }))
        .answer("zephyr_get_test_case", json!({ "key": "BIG-T1", "name": "Login" }));
    let answer = call(&http, actions::CASES, json!({ "issue": "BIG-1" })).await;
    assert_eq!(answer, Ok(json!([{ "ref": "BIG-T1", "title": "Login" }])));
}

#[tokio::test]
async fn a_result_is_recorded_with_the_remote_status_and_milliseconds() {
    let http = ZephyrMcp::new().answer("zephyr_create_test_execution", json!({ "id": 99, "self": "https://api/99" }));
    let args = json!({ "case": "OPS-T3", "cycle": "OPS-R1", "status": "not_run", "comment": "c", "seconds": 1.5 });
    let answer = Zephyr.call(&http, &creds(json!({})), actions::RECORD, &args, "").await;
    assert_eq!(answer, Ok(json!({ "ref": "99" })));
    assert_eq!(
        http.calls()[0].1,
        json!({
            "projectKey": "OPS", "testCaseKey": "OPS-T3", "testCycleKey": "OPS-R1",
            "statusName": "Not Executed", "comment": "c", "executionTime": 1500,
        })
    );
    for (status, remote) in [("passed", "Pass"), ("failed", "Fail"), ("blocked", "Blocked")] {
        let http = ZephyrMcp::new()
            .answer("zephyr_get_test_executions", json!({ "values": [], "nextStartAtId": null, "limit": 50 }))
            .answer("zephyr_create_test_execution", json!({ "id": 1 }));
        let args = json!({ "case": "BIG-T1", "cycle": "BIG-R1", "status": status });
        assert!(call(&http, actions::RECORD, args).await.is_ok());
        assert_eq!(http.calls()[1].1["statusName"], remote);
    }
    let http = ZephyrMcp::new();
    let unknown = call(&http, actions::RECORD, json!({ "case": "BIG-T1", "cycle": "BIG-R1", "status": "ok" })).await;
    assert_eq!(unknown.map_err(|error| error.class), Err(ErrorClass::Permanent));
    assert!(http.calls().is_empty());
}

#[tokio::test]
async fn a_cycle_is_created_and_linked_to_the_issue() {
    let http = ZephyrMcp::new()
        .answer("zephyr_create_test_cycle", json!({ "id": 5, "key": "BIG-R5" }))
        .answer("zephyr_create_test_cycle_issue_link", json!({ "id": 6 }));
    let args = json!({ "title": "Run 1", "issue_id": 42 });
    let answer = Zephyr.call(&http, &creds(json!({ "project": "BIG" })), actions::CREATE_CYCLE, &args, "").await;
    assert_eq!(answer, Ok(json!({ "ref": "BIG-R5", "url": "" })));
    let calls = http.calls();
    assert_eq!(calls[0].1, json!({ "projectKey": "BIG", "name": "Run 1" }));
    assert_eq!(calls[1].1, json!({ "testCycleIdOrKey": "BIG-R5", "issueId": 42 }));
}

#[tokio::test]
async fn without_a_project_a_case_is_blocked_before_any_write() {
    let http = ZephyrMcp::new();
    let refused = Zephyr.call(&http, &creds(json!({})), actions::CREATE_CASE, &json!({ "title": "T" }), "k").await;
    assert_eq!(refused.as_ref().map_err(ActionError::blocker), Err(Some(NO_PROJECT)));
    assert!(http.calls().is_empty());
}

#[tokio::test]
async fn without_a_chosen_project_the_case_goes_to_the_project_of_its_issue() {
    let http = ZephyrMcp::new()
        .answer("zephyr_get_issue_link_test_cases", json!({ "result": [] }))
        .answer("zephyr_create_test_case", json!({ "key": "DEMO-T1" }));
    let args = json!({ "title": "T", "issue": "DEMO-17" });
    let made = Zephyr.call(&http, &creds(json!({})), actions::CREATE_CASE, &args, "").await.expect("made");
    assert_eq!(made["ref"], "DEMO-T1");
    assert_eq!(http.calls()[1].1["projectKey"], "DEMO");
}

#[tokio::test]
async fn a_case_goes_into_the_folder_of_its_issue_made_once() {
    let folders = json!({ "values": [{ "id": 3, "name": "DEMO-17", "parentId": 9 }], "isLast": true });
    let http = ZephyrMcp::new()
        .answer("zephyr_get_issue_link_test_cases", json!({ "result": [] }))
        .answer("zephyr_get_folders", folders)
        .answer("zephyr_create_folder", json!({ "id": 41, "self": "https://api/folders/41" }))
        .answer("zephyr_create_test_case", json!({ "key": "DEMO-T1" }));
    let args = json!({ "title": "T", "issue": "DEMO-17", "folder": "DEMO-17" });
    Zephyr.call(&http, &creds(json!({})), actions::CREATE_CASE, &args, "").await.expect("made");
    let calls = http.calls();
    assert_eq!(calls[2].1, json!({ "projectKey": "DEMO", "name": "DEMO-17", "folderType": "TEST_CASE" }));
    assert_eq!(calls[3].1["folderId"], 41);

    let found = json!({ "values": [{ "id": 41, "name": "DEMO-17", "parentId": null }], "isLast": true });
    let http = ZephyrMcp::new()
        .answer("zephyr_get_issue_link_test_cases", json!({ "result": [] }))
        .answer("zephyr_get_folders", found)
        .answer("zephyr_create_test_case", json!({ "key": "DEMO-T2" }));
    Zephyr.call(&http, &creds(json!({})), actions::CREATE_CASE, &args, "").await.expect("made");
    let names: Vec<String> = http.calls().into_iter().map(|(name, _)| name).collect();
    assert!(!names.contains(&"zephyr_create_folder".to_owned()), "{names:?}");
    assert_eq!(http.calls()[2].1["folderId"], 41);
}

#[tokio::test]
async fn projects_are_paged_filtered_and_the_only_one_becomes_the_account_project() {
    let first: Vec<Value> = (0..50).map(|n| json!({ "id": n, "key": format!("P{n}") })).collect();
    let http = ZephyrMcp::new()
        .answer("zephyr_get_projects", json!({ "values": first, "isLast": false }))
        .answer("zephyr_get_projects", json!({ "values": [{ "id": 77, "key": "APP" }], "isLast": true }));
    let answer = call(&http, actions::PROJECTS, json!({ "query": "app" })).await;
    assert_eq!(answer, Ok(json!([{ "id": "77", "key": "APP", "name": "APP" }])));
    assert_eq!(http.calls()[1].1, json!({ "startAt": 50, "maxResults": 50 }));
    let http = ZephyrMcp::new().answer("zephyr_get_projects", json!({ "values": [{ "id": 1, "key": "BIG" }] }));
    let options = Zephyr.accounts(&http, &creds(json!({}))).await;
    let Ok(options) = options else { panic!("{options:?}") };
    assert_eq!(options[0].settings, json!({ "project": "BIG", "detail": "Projeto BIG" }));
}

#[test]
fn the_smartbear_sign_in_is_the_default_and_an_api_token_the_alternative() {
    let AuthSpec::Loopback(_) = Zephyr.descriptor().auth else { panic!("not loopback") };
    let [alternative] = Zephyr.alternatives().try_into().unwrap_or_else(|_| panic!("not one alternative"));
    let AuthSpec::Token(spec) = alternative.auth else { panic!("not a token") };
    assert_eq!((alternative.key.as_str(), alternative.label.as_str()), (API_TOKEN, "Usar API token"));
    assert_eq!(spec.fields[0].name, "token");
    assert!(spec.fields[0].help.contains("Zephyr API Access Tokens"));
    assert_eq!(spec.access_token(&json!({ "token": " zt-1 " })), Ok("zt-1".to_owned()));
}

fn no_more(field: &str) -> Value {
    json!({ "values": [], field: null, "limit": 50, "isLast": true })
}

#[tokio::test]
async fn a_retried_cycle_with_the_same_key_is_found_by_its_marker_and_not_created_again() {
    let first = ZephyrMcp::new()
        .answer("zephyr_get_test_cycles", no_more("next"))
        .answer("zephyr_create_test_cycle", json!({ "id": 5, "key": "BIG-R5" }));
    let made = call(&first, actions::CREATE_CYCLE, json!({ "title": "Run 1", "description": "d" })).await;
    assert_eq!(made, Ok(json!({ "ref": "BIG-R5", "url": "" })));
    let calls = first.calls();
    assert_eq!(calls[0].1, json!({ "projectKey": "BIG", "maxResults": 1000, "startAt": 0 }));
    let stored = calls[1].1["description"].as_str().unwrap_or_default().to_owned();
    assert!(stored.starts_with('d') && idempotency::carries_tag(&stored, "k1") && !stored.contains('<'), "{stored}");

    let other = json!({ "id": 4, "key": "BIG-R4", "name": "Run 1", "description": idempotency::marked("d", "k0") });
    let mine = json!({ "id": 5, "key": "BIG-R5", "name": "Run 1", "description": format!("<p>{stored}</p>") });
    let retried = ZephyrMcp::new()
        .answer("zephyr_get_test_cycles", json!({ "values": [other], "isLast": false }))
        .answer("zephyr_get_test_cycles", json!({ "values": [mine], "isLast": true }));
    let again = call(&retried, actions::CREATE_CYCLE, json!({ "title": "Run 1", "description": "d" })).await;
    assert_eq!(again, Ok(json!({ "ref": "BIG-R5", "url": "" })));
    let calls = retried.calls();
    assert_eq!(calls[1].1["startAt"], 1);
    assert!(calls.iter().all(|(name, _)| !name.starts_with("zephyr_create")), "{calls:?}");
}

#[tokio::test]
async fn a_retried_result_with_the_same_key_is_found_by_its_marker_and_not_recorded_again() {
    let args = json!({ "case": "BIG-T1", "cycle": "BIG-R1", "status": "passed" });
    let first = ZephyrMcp::new()
        .answer("zephyr_get_test_executions", no_more("nextStartAtId"))
        .answer("zephyr_create_test_execution", json!({ "id": 9, "key": "BIG-E9" }));
    assert_eq!(call(&first, actions::RECORD, args.clone()).await, Ok(json!({ "ref": "BIG-E9" })));
    let calls = first.calls();
    assert_eq!(
        calls[0].1,
        json!({ "projectKey": "BIG", "testCycle": "BIG-R1", "testCase": "BIG-T1", "limit": 50 })
    );
    let stored = calls[1].1["comment"].as_str().unwrap_or_default().to_owned();
    assert!(idempotency::carries_tag(&stored, "k1") && !stored.starts_with('\n'), "{stored}");

    let other = json!({ "id": 8, "key": "BIG-E8", "comment": null });
    let mine = json!({ "id": 9, "key": "BIG-E9", "comment": stored });
    let retried = ZephyrMcp::new()
        .answer("zephyr_get_test_executions", json!({ "values": [other], "nextStartAtId": 9, "limit": 1 }))
        .answer("zephyr_get_test_executions", json!({ "values": [mine], "nextStartAtId": null, "limit": 1 }));
    assert_eq!(call(&retried, actions::RECORD, args).await, Ok(json!({ "ref": "BIG-E9" })));
    let calls = retried.calls();
    assert_eq!(calls[1].1["startAtId"], 9);
    assert!(calls.iter().all(|(name, _)| !name.starts_with("zephyr_create")), "{calls:?}");
}

#[tokio::test]
async fn a_retried_case_with_the_same_key_is_found_by_its_label_and_not_created_again() {
    let args = json!({ "title": "Login works", "labels": ["auto"] });
    let first = ZephyrMcp::new()
        .answer("zephyr_get_test_cases", no_more("nextStartAtId"))
        .answer("zephyr_create_test_case", json!({ "id": 7, "key": "BIG-T7" }));
    let made = call(&first, actions::CREATE_CASE, args.clone()).await;
    assert_eq!(made, Ok(json!({ "ref": "BIG-T7", "title": "Login works", "url": "", "created": true })));
    let stored = first.calls()[1].1["labels"].clone();
    assert_eq!(stored, json!(["auto", idempotency::label("k1")]));

    let other = json!({ "id": 6, "key": "BIG-T6", "name": "Login works", "labels": ["auto"] });
    let mine = json!({ "id": 7, "key": "BIG-T7", "name": "Login works", "labels": stored });
    let retried = ZephyrMcp::new()
        .answer("zephyr_get_test_cases", json!({ "values": [other], "nextStartAtId": 7, "limit": 1 }))
        .answer("zephyr_get_test_cases", json!({ "values": [mine], "nextStartAtId": null, "limit": 1 }));
    let again = call(&retried, actions::CREATE_CASE, args).await;
    assert_eq!(again, Ok(json!({ "ref": "BIG-T7", "title": "Login works", "url": "", "created": false })));
    let calls = retried.calls();
    assert_eq!(calls[1].1["startAtId"], 7);
    assert!(calls.iter().all(|(name, _)| !name.starts_with("zephyr_create")), "{calls:?}");
}
