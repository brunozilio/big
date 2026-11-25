use domain::Language;
use domain::connection::Capability;
use serde_json::{Value, json};

use super::{creds, loaded};
use crate::descriptor::{AuthSpec, TokenForm};
use crate::error::{ActionError, ErrorClass};
use crate::http::{FakeHttp, Method};
use crate::provider::Provider;

async fn call(http: &FakeHttp, settings: Value, action: &str, args: Value) -> Result<Value, ActionError> {
    let provider = loaded("sonarqube");
    let answer = provider.call(http, &creds("squ_token", settings), action, &args, "k").await;
    if let Ok(value) = &answer {
        let spec = provider.descriptor().action(action).cloned().unwrap_or_else(|| panic!("no {action}"));
        assert_eq!(crate::descriptor::conforms(&spec.output_schema, value), Ok(()), "{action}: {value}");
    }
    answer
}

const GATE: &str = r#"{"projectStatus":{"status":"ERROR","conditions":[
    {"status":"ERROR","metricKey":"new_coverage","comparator":"LT","errorThreshold":"80","actualValue":"71.4"},
    {"status":"OK","metricKey":"new_duplicated_lines_density","comparator":"GT","errorThreshold":"3","actualValue":"1.2"}]}}"#;

const METRICS: &str = r#"{"metrics":[{"key":"new_coverage","name":"Coverage on New Code"}]}"#;

const MEASURES: &str = r#"{"component":{"measures":[
    {"metric":"new_reliability_rating","period":{"value":"3.0"}},
    {"metric":"new_security_rating","period":{"value":"1.0"}},
    {"metric":"new_bugs","period":{"value":"2"}},
    {"metric":"new_coverage","period":{"value":"71.4"}}]}}"#;

const ISSUES: &str = r#"{"total":7,"issues":[
    {"key":"AX1","type":"BUG","severity":"MAJOR","message":"Null check","component":"acme_web:src/cart.ts","line":12}]}"#;

#[test]
fn the_provider_signs_in_with_an_address_an_organization_and_a_token_and_binds_repositories() {
    let provider = loaded("sonarqube");
    let descriptor = provider.descriptor();
    let AuthSpec::Token(spec) = descriptor.auth else { panic!("not a token") };
    let names: Vec<(&str, bool, bool)> =
        spec.fields.iter().map(|field| (field.name.as_str(), field.secret, field.setting)).collect();
    assert_eq!(names, [("url", false, true), ("organization", false, true), ("token", true, false)]);
    assert_eq!(spec.fields[0].value, "https://sonarcloud.io");
    assert_eq!(spec.form, TokenForm::Bearer);
    assert_eq!(descriptor.capabilities, vec![Capability::from("quality")]);
    let actions: Vec<&str> = descriptor.actions.iter().map(|action| action.name.as_str()).collect();
    assert_eq!(actions, ["quality.report", "quality.options"]);
    let look = provider.presentation(Language::Portuguese);
    assert_eq!(look.group, "quality");
    let bound: Vec<(&str, &str)> =
        look.bindings.iter().map(|binding| (binding.capability.as_str(), binding.scope.as_str())).collect();
    assert_eq!(bound, [("quality", "repository")]);
}

#[tokio::test]
async fn the_report_of_a_pull_request_carries_the_gate_the_new_code_measures_and_its_issues() {
    let http = FakeHttp::new()
        .once(Method::Get, "/api/qualitygates/project_status", 200, GATE)
        .once(Method::Get, "/api/metrics/search", 200, METRICS)
        .once(Method::Get, "/api/measures/component", 200, MEASURES)
        .once(Method::Get, "/api/issues/search", 200, ISSUES);
    let settings = json!({ "url": "https://sonar.acme.test/" });
    let found = call(&http, settings, "quality.report", json!({ "project": "acme_web", "pullRequest": "12" })).await;
    let Ok(found) = found else { panic!("{found:?}") };
    assert_eq!((&found["status"], &found["gate"]), (&json!("done"), &json!("failed")));
    assert_eq!(found["ratings"], json!({ "reliability": "C", "security": "A", "maintainability": "A" }));
    assert_eq!((&found["counts"]["bugs"], &found["coverage"], &found["duplication"]), (&json!(2), &json!(71.4), &Value::Null));
    assert_eq!(
        found["conditions"][0],
        json!({ "metric": "new_coverage", "label": "Coverage on New Code", "value": "71.4", "threshold": "80", "passed": false })
    );
    assert_eq!(found["issues"][0]["file"], "src/cart.ts");
    assert_eq!((&found["issues"][0]["type"], &found["issues"][0]["severity"]), (&json!("bug"), &json!("major")));
    assert_eq!(found["total"], 7);
    assert_eq!(found["conditions"][1]["label"], "new_duplicated_lines_density");
    assert_eq!(found["url"], "https://sonar.acme.test/dashboard?id=acme_web&pullRequest=12");
    let sent = http.sent();
    assert_eq!(sent[0].url, "https://sonar.acme.test/api/qualitygates/project_status?projectKey=acme_web&pullRequest=12");
    assert_eq!(sent[0].header_value("authorization"), Some("Bearer squ_token"));
}

#[tokio::test]
async fn a_pull_request_not_analyzed_yet_is_pending() {
    let http = FakeHttp::new().once(Method::Get, "/api/qualitygates/project_status", 404, r#"{"errors":[]}"#);
    let found = call(&http, json!({}), "quality.report", json!({ "project": "acme_web", "pullRequest": "12" })).await;
    assert_eq!(found.map(|found| found["status"].clone()), Ok(json!("pending")));
    let http =
        FakeHttp::new().once(Method::Get, "/api/qualitygates/project_status", 200, r#"{"projectStatus":{"status":"NONE"}}"#);
    let found = call(&http, json!({}), "quality.report", json!({ "project": "acme_web", "branch": "main" })).await;
    assert_eq!(found.map(|found| found["status"].clone()), Ok(json!("pending")));
    assert!(http.sent()[0].url.starts_with("https://sonarcloud.io/api/qualitygates/project_status"));
}

#[tokio::test]
async fn an_analysis_of_an_older_commit_is_still_pending() {
    let pulls = r#"{"pullRequests":[{"key":"12","commit":{"sha":"aaa111"}},{"key":"13","commit":{"sha":"bbb222"}}]}"#;
    let http = FakeHttp::new().once(Method::Get, "/api/project_pull_requests/list", 200, pulls);
    let args = json!({ "project": "acme_web", "pullRequest": "12", "commit": "ccc333" });
    assert_eq!(call(&http, json!({}), "quality.report", args).await.map(|found| found["status"].clone()), Ok(json!("pending")));
    let http = FakeHttp::new()
        .once(Method::Get, "/api/project_pull_requests/list", 200, pulls)
        .once(Method::Get, "/api/qualitygates/project_status", 200, GATE)
        .once(Method::Get, "/api/metrics/search", 200, METRICS)
        .once(Method::Get, "/api/measures/component", 200, MEASURES)
        .once(Method::Get, "/api/issues/search", 200, ISSUES);
    let args = json!({ "project": "acme_web", "pullRequest": "12", "commit": "aaa111ffff" });
    assert_eq!(call(&http, json!({}), "quality.report", args).await.map(|found| found["gate"].clone()), Ok(json!("failed")));
}

#[tokio::test]
async fn the_projects_to_bind_are_listed_in_the_organization() {
    let http = FakeHttp::new().once(
        Method::Get,
        "/api/components/search",
        200,
        r#"{"components":[{"key":"acme_web","name":"Web"},{"key":"acme_api","name":"acme_api"}]}"#,
    );
    let found = call(&http, json!({ "organization": "acme" }), "quality.options", json!({ "query": "acme" })).await;
    assert_eq!(
        found,
        Ok(json!([{ "value": "acme_web", "label": "Web · acme_web" }, { "value": "acme_api", "label": "acme_api" }]))
    );
    let url = &http.sent()[0].url;
    assert!(url.contains("qualifiers=TRK") && url.contains("q=acme") && url.contains("organization=acme"), "{url}");
}

#[tokio::test]
async fn a_token_sonarqube_refuses_is_an_auth_error_and_a_valid_one_names_the_account() {
    let http = FakeHttp::new().once(Method::Get, "/api/authentication/validate", 200, r#"{"valid":false}"#);
    let refused = loaded("sonarqube").accounts(&http, &creds("bad", json!({}))).await;
    assert_eq!(refused.map_err(|error| error.class), Err(ErrorClass::Auth));
    let http = FakeHttp::new()
        .once(Method::Get, "/api/authentication/validate", 200, r#"{"valid":true}"#)
        .once(Method::Get, "/api/users/current", 200, r#"{"login":"ana"}"#);
    let found = loaded("sonarqube").accounts(&http, &creds("squ", json!({ "organization": "acme" }))).await;
    let Ok(found) = found else { panic!("{found:?}") };
    assert_eq!(found[0].label, "ana · acme");
}
