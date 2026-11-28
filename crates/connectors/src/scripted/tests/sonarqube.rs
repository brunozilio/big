use domain::Language;
use domain::connection::Capability;
use serde_json::{Value, json};

use super::{creds, loaded, now};
use crate::descriptor::{AuthSpec, TokenForm};
use crate::error::{ActionError, ErrorClass};
use crate::http::{FakeHttp, Method};
use crate::oauth::{Authorization, Loopback};
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

fn sign_in() -> Loopback {
    let AuthSpec::Loopback(sign_in) = loaded("sonarqube").descriptor().auth else { panic!("not loopback") };
    sign_in
}

#[test]
fn the_provider_signs_in_through_the_browser_or_with_an_address_and_a_token() {
    let provider = loaded("sonarqube");
    let descriptor = provider.descriptor();
    let sign_in = sign_in();
    assert_eq!((sign_in.0.callback_ports(), sign_in.0.listens()), (Some((64120, 64130)), true));
    let entered: Vec<String> = sign_in.0.fields(Language::Portuguese).into_iter().map(|field| field.name).collect();
    assert_eq!(entered, ["url"]);
    let [alternate] = provider.alternatives(Language::Portuguese).try_into().unwrap_or_else(|_| panic!("one"));
    let AuthSpec::Token(spec) = alternate.auth else { panic!("not a token") };
    let names: Vec<(&str, bool, bool)> =
        spec.fields.iter().map(|field| (field.name.as_str(), field.secret, field.setting)).collect();
    assert_eq!(names, [("url", false, true), ("token", true, false)]);
    assert_eq!(spec.fields[0].value, "https://sonarcloud.io");
    let optional: Vec<bool> = spec.fields.iter().map(|field| field.optional).collect();
    assert_eq!(optional, [false, false]);
    let given = json!({ "url": "https://sonar.empresa.com", "token": "squ" });
    assert_eq!(spec.access_token(&given, Language::Portuguese), Ok("squ".to_owned()));
    assert_eq!(spec.settings(&given), json!({ "url": "https://sonar.empresa.com" }));
    assert_eq!(spec.form, TokenForm::Bearer);
    assert_eq!(descriptor.capabilities, vec![Capability::from("quality")]);
    let actions: Vec<&str> = descriptor.actions.iter().map(|action| action.name.as_str()).collect();
    assert_eq!(actions, ["quality.report", "quality.options"]);
    let look = provider.presentation(Language::Portuguese);
    assert_eq!(look.group, "quality");
    let bound: Vec<(&str, &str)> =
        look.bindings.iter().map(|binding| (binding.capability.as_str(), binding.scope.as_str())).collect();
    assert_eq!(bound, [("quality", "repository")]);
    assert!(look.bindings[0].suggests);
    let summary = look.bindings[0].summary.clone().unwrap_or_default();
    assert_eq!((summary.parts, summary.separator.as_str()), (vec!["{label}".to_owned()], " · "));
}

async fn begun(http: &FakeHttp) -> (Loopback, Authorization) {
    let entered = json!({ "url": "sonar.empresa.com/sonar/" });
    let sign_in = sign_in().entered(&entered, Language::Portuguese).unwrap_or_else(|error| panic!("{error}"));
    let begun = sign_in.0.begin_at(http, 7420, Some(64121)).await.unwrap_or_else(|error| panic!("{error:?}"));
    (sign_in, begun)
}

#[tokio::test]
async fn the_browser_sign_in_opens_the_token_page_with_the_port_that_listens() {
    let http = FakeHttp::new();
    let (_, begun) = begun(&http).await;
    let ide = url::form_urlencoded::byte_serialize(brand::NAME.as_bytes()).collect::<String>().replace('+', "%20");
    let expected = format!("https://sonar.empresa.com/sonar/sonarlint/auth?ideName={ide}&port=64121");
    assert_eq!(begun.url, expected);
    assert!(!begun.state.is_empty());
    let without = sign_in().0.begin_at(&http, 7420, None).await;
    assert!(without.is_err(), "{without:?}");
}

#[tokio::test]
async fn the_listening_port_says_it_needs_a_token_and_takes_the_token_the_page_posts() {
    let http = FakeHttp::new();
    let (sign_in, begun) = begun(&http).await;
    let asked = |method: &str, path: &str, body: Value| json!({ "method": method, "path": path, "query": {}, "origin": "https://sonar.empresa.com", "body": body });
    let ask = |request: Value| {
        let sign_in = sign_in.clone();
        let begun = begun.clone();
        let http = &http;
        async move { sign_in.0.answer(http, &begun, &request, now()).await.unwrap_or_else(|error| panic!("{error:?}")) }
    };
    let status = ask(asked("GET", "/sonarlint/api/status", Value::Null)).await;
    assert_eq!((status.status, status.body["needsToken"].clone(), status.grant), (200, json!(true), None));
    assert_eq!(status.body["ideName"], brand::NAME);
    let given = ask(asked("POST", "/sonarlint/api/token", json!({ "token": "squ_1" }))).await;
    let grant = given.grant.unwrap_or_else(|| panic!("no grant"));
    assert_eq!((given.status, grant.access_token.as_str(), grant.refresh_token), (200, "squ_1", None));
    let empty = ask(asked("POST", "/sonarlint/api/token", json!({}))).await;
    assert_eq!((empty.status, empty.grant), (400, None));
    let other = ask(asked("GET", "/elsewhere", Value::Null)).await;
    assert_eq!((other.status, other.grant), (404, None));
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
    assert_eq!(
        (&found["counts"]["bugs"], &found["coverage"], &found["duplication"]),
        (&json!(2), &json!(71.4), &Value::Null)
    );
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
    assert_eq!(
        sent[0].url,
        "https://sonar.acme.test/api/qualitygates/project_status?projectKey=acme_web&pullRequest=12"
    );
    assert_eq!(sent[0].header_value("authorization"), Some("Bearer squ_token"));
}

#[tokio::test]
async fn a_pull_request_not_analyzed_yet_is_pending() {
    let http = FakeHttp::new().once(Method::Get, "/api/qualitygates/project_status", 404, r#"{"errors":[]}"#);
    let found = call(&http, json!({}), "quality.report", json!({ "project": "acme_web", "pullRequest": "12" })).await;
    assert_eq!(found.map(|found| found["status"].clone()), Ok(json!("pending")));
    let http = FakeHttp::new().once(
        Method::Get,
        "/api/qualitygates/project_status",
        200,
        r#"{"projectStatus":{"status":"NONE"}}"#,
    );
    let found = call(&http, json!({}), "quality.report", json!({ "project": "acme_web", "branch": "main" })).await;
    assert_eq!(found.map(|found| found["status"].clone()), Ok(json!("pending")));
    assert!(http.sent()[0].url.starts_with("https://sonarcloud.io/api/qualitygates/project_status"));
}

#[tokio::test]
async fn an_analysis_of_an_older_commit_is_still_pending() {
    let pulls = r#"{"pullRequests":[{"key":"12","commit":{"sha":"aaa111"}},{"key":"13","commit":{"sha":"bbb222"}}]}"#;
    let http = FakeHttp::new().once(Method::Get, "/api/project_pull_requests/list", 200, pulls);
    let args = json!({ "project": "acme_web", "pullRequest": "12", "commit": "ccc333" });
    assert_eq!(
        call(&http, json!({}), "quality.report", args).await.map(|found| found["status"].clone()),
        Ok(json!("pending"))
    );
    let http = FakeHttp::new()
        .once(Method::Get, "/api/project_pull_requests/list", 200, pulls)
        .once(Method::Get, "/api/qualitygates/project_status", 200, GATE)
        .once(Method::Get, "/api/metrics/search", 200, METRICS)
        .once(Method::Get, "/api/measures/component", 200, MEASURES)
        .once(Method::Get, "/api/issues/search", 200, ISSUES);
    let args = json!({ "project": "acme_web", "pullRequest": "12", "commit": "aaa111ffff" });
    assert_eq!(
        call(&http, json!({}), "quality.report", args).await.map(|found| found["gate"].clone()),
        Ok(json!("failed"))
    );
}

#[tokio::test]
async fn the_suggestion_for_a_repository_is_the_project_keyed_by_its_name_with_or_without_the_owner() {
    let http = FakeHttp::new().once(Method::Get, "/api/components/search", 200, r#"{"components":[]}"#).once(
        Method::Get,
        "/api/components/search",
        200,
        r#"{"components":[{"key":"acme_webapp"},{"key":"acme_web"}]}"#,
    );
    let names = json!({ "names": ["acme/web", "web"] });
    let found = call(&http, json!({ "organization": "acme" }), "quality.options", names).await;
    assert_eq!(found, Ok(json!([{ "value": "acme_web", "label": "acme_web" }])));
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
        Ok(json!([{ "value": "acme_web", "label": "acme_web" }, { "value": "acme_api", "label": "acme_api" }]))
    );
    let url = &http.sent()[0].url;
    assert!(url.contains("qualifiers=TRK") && url.contains("q=acme") && url.contains("organization=acme"), "{url}");
}

#[tokio::test]
async fn every_page_of_projects_is_listed_until_the_total() {
    let page = |keys: &[&str]| {
        let components: Vec<_> = keys.iter().map(|key| json!({ "key": key, "name": "Name" })).collect();
        json!({ "paging": { "pageIndex": 1, "pageSize": 500, "total": 1000 }, "components": components }).to_string()
    };
    let http = FakeHttp::new().once(Method::Get, "/api/components/search", 200, &page(&["a", "b"])).once(
        Method::Get,
        "/api/components/search",
        200,
        &page(&["c"]),
    );
    let found = call(&http, json!({ "url": "https://sonar.acme.test" }), "quality.options", json!({})).await;
    let Ok(found) = found else { panic!("{found:?}") };
    let keys: Vec<_> =
        found.as_array().map(|listed| listed.iter().map(|o| o["label"].clone()).collect()).unwrap_or_default();
    assert_eq!(keys, [json!("a"), json!("b"), json!("c")]);
}

#[tokio::test]
async fn a_token_sonarqube_refuses_is_an_auth_error_and_a_valid_one_names_the_account() {
    let http = FakeHttp::new().always(Method::Get, "/api/authentication/validate", 200, r#"{"valid":false}"#);
    let refused = loaded("sonarqube").accounts(&http, &creds("bad", json!({ "organization": "acme" }))).await;
    assert_eq!(refused.map_err(|error| error.class), Err(ErrorClass::Auth));
    let http = FakeHttp::new().once(Method::Get, "/api/authentication/validate", 200, r#"{"valid":true}"#).once(
        Method::Get,
        "/api/users/current",
        200,
        r#"{"login":"ana","isLoggedIn":true}"#,
    );
    let found = loaded("sonarqube").accounts(&http, &creds("squ", json!({ "organization": "acme" }))).await;
    let Ok(found) = found else { panic!("{found:?}") };
    assert_eq!(found[0].label, "ana · acme");
}

fn signed_in() -> FakeHttp {
    FakeHttp::new().once(Method::Get, "/api/authentication/validate", 200, r#"{"valid":true}"#).once(
        Method::Get,
        "/api/users/current",
        200,
        r#"{"login":"ana","isLoggedIn":true}"#,
    )
}

#[tokio::test]
async fn sonarqube_cloud_connects_the_account_without_asking_for_an_organization() {
    let found = loaded("sonarqube").accounts(&signed_in(), &creds("squ", json!({}))).await;
    let Ok(found) = found else { panic!("{found:?}") };
    let offered: Vec<(&str, &Value)> = found.iter().map(|option| (option.label.as_str(), &option.settings)).collect();
    assert_eq!(offered, [("ana · sonarcloud.io", &json!({ "url": "https://sonarcloud.io" }))]);
}

#[tokio::test]
async fn sonarqube_cloud_lists_the_projects_of_every_organization_the_user_belongs_to() {
    let listed = r#"{"organizations":[{"key":"acme"},{"key":"beta"}],"paging":{"total":2}}"#;
    let acme = r#"{"components":[{"key":"acme_web","name":"web"}],"paging":{"total":1}}"#;
    let beta = r#"{"components":[{"key":"beta_api","name":"api"}],"paging":{"total":1}}"#;
    let http = FakeHttp::new()
        .once(Method::Get, "/api/organizations/search", 200, listed)
        .once(Method::Get, "/api/components/search", 200, acme)
        .once(Method::Get, "/api/components/search", 200, beta);
    let found = call(&http, json!({}), "quality.options", json!({})).await;
    let Ok(found) = found else { panic!("{found:?}") };
    assert_eq!(
        found,
        json!([{ "value": "acme_web", "label": "acme · acme_web" }, { "value": "beta_api", "label": "beta · beta_api" }])
    );
    let sent: Vec<String> = http.sent().iter().map(|request| request.url.clone()).collect();
    assert!(sent[0].contains("/api/organizations/search?") && sent[0].contains("member=true"), "{sent:?}");
    assert!(sent[1].contains("organization=acme") && sent[2].contains("organization=beta"), "{sent:?}");
}

#[tokio::test]
async fn a_self_hosted_server_with_a_context_path_signs_in_without_an_organization() {
    let http = FakeHttp::new().once(Method::Get, "/api/authentication/validate", 200, r#"{"valid":true}"#).once(
        Method::Get,
        "/api/users/current",
        200,
        r#"{"login":"ana","isLoggedIn":true}"#,
    );
    let settings = json!({ "url": "sonar.acme.test/sonar/", "organization": "ignored" });
    let found = loaded("sonarqube").accounts(&http, &creds("squ", settings)).await;
    let Ok(found) = found else { panic!("{found:?}") };
    assert_eq!(found[0].label, "ana · sonar.acme.test/sonar");
    assert_eq!(found[0].settings, json!({ "url": "https://sonar.acme.test/sonar" }));
    let sent = http.sent();
    assert_eq!(sent[0].url, "https://sonar.acme.test/sonar/api/authentication/validate");
    assert_eq!(sent[1].url, "https://sonar.acme.test/sonar/api/users/current");
    assert_eq!(sent[0].header_value("authorization"), Some("Bearer squ"));
}

#[tokio::test]
async fn an_older_server_that_ignores_bearer_signs_in_with_the_token_as_basic_login() {
    let http = FakeHttp::new()
        .once(Method::Get, "/api/authentication/validate", 200, r#"{"valid":false}"#)
        .once(Method::Get, "/api/authentication/validate", 200, r#"{"valid":true}"#)
        .once(Method::Get, "/api/users/current", 200, r#"{"login":"ana","isLoggedIn":true}"#);
    let found = loaded("sonarqube").accounts(&http, &creds("squ", json!({ "url": "http://10.0.0.5:9000" }))).await;
    let Ok(found) = found else { panic!("{found:?}") };
    assert_eq!(found[0].label, "ana · 10.0.0.5:9000");
    assert_eq!(http.sent()[1].header_value("authorization"), Some("Basic c3F1Og=="));
}

#[tokio::test]
async fn a_wrong_address_names_the_path_and_the_status() {
    let http = FakeHttp::new().always(Method::Get, "/api/authentication/validate", 404, "<html>Not Found</html>");
    let failed = loaded("sonarqube").accounts(&http, &creds("squ", json!({ "url": "https://acme.test" }))).await;
    let Err(failed) = failed else { panic!("{failed:?}") };
    assert!(failed.message.contains("404"), "{}", failed.message);
    assert!(failed.message.contains("https://acme.test/api/authentication/validate"), "{}", failed.message);
    assert!(failed.message.contains("context path"), "{}", failed.message);
    let http = FakeHttp::new().always(Method::Get, "/api/authentication/validate", 200, "<html>login</html>");
    let failed = loaded("sonarqube").accounts(&http, &creds("squ", json!({ "url": "https://acme.test" }))).await;
    let Err(failed) = failed else { panic!("{failed:?}") };
    assert!(failed.message.contains("not JSON"), "{}", failed.message);
}

#[tokio::test]
async fn a_self_hosted_server_lists_projects_and_reports_without_an_organization() {
    let http = FakeHttp::new().once(
        Method::Get,
        "/api/components/search",
        200,
        r#"{"components":[{"key":"web","name":"Web"}]}"#,
    );
    let settings = json!({ "url": "https://sonar.acme.test/sonar", "organization": "acme" });
    let found = call(&http, settings.clone(), "quality.options", json!({})).await;
    assert_eq!(found, Ok(json!([{ "value": "web", "label": "web" }])));
    let url = &http.sent()[0].url;
    assert!(
        url.starts_with("https://sonar.acme.test/sonar/api/components/search?") && !url.contains("organization"),
        "{url}"
    );
    let impacts = r#"{"total":1,"issues":[{"key":"AX2","impacts":[{"softwareQuality":"SECURITY","severity":"HIGH"}],
        "message":"Injection","component":"web:src/db.ts","line":3}]}"#;
    let http = FakeHttp::new()
        .once(Method::Get, "/api/project_pull_requests/list", 404, r#"{"errors":[{"msg":"Unknown url"}]}"#)
        .once(
            Method::Get,
            "/api/qualitygates/project_status",
            200,
            r#"{"projectStatus":{"status":"OK","conditions":[]}}"#,
        )
        .once(
            Method::Get,
            "/api/measures/component",
            404,
            r#"{"errors":[{"msg":"The following metric keys are not found: new_bugs, new_code_smells"}]}"#,
        )
        .once(Method::Get, "/api/measures/component", 200, MEASURES)
        .once(Method::Get, "/api/issues/search", 200, impacts);
    let args = json!({ "project": "web", "pullRequest": "4", "commit": "abc" });
    let found = call(&http, settings, "quality.report", args).await;
    let Ok(found) = found else { panic!("{found:?}") };
    assert_eq!((&found["gate"], &found["ratings"]["reliability"]), (&json!("passed"), &json!("C")));
    assert_eq!(
        (&found["issues"][0]["type"], &found["issues"][0]["severity"]),
        (&json!("vulnerability"), &json!("critical"))
    );
    assert_eq!(found["url"], "https://sonar.acme.test/sonar/dashboard?id=web&pullRequest=4");
    let retried = &http.sent_to(Method::Get, "/api/measures/component")[1].url;
    assert!(!retried.contains("new_bugs") && retried.contains("new_coverage"), "{retried}");
}

#[tokio::test]
async fn a_call_refused_with_bearer_is_retried_with_the_token_as_basic_login() {
    let http = FakeHttp::new().once(Method::Get, "/api/components/search", 401, "").once(
        Method::Get,
        "/api/components/search",
        200,
        r#"{"components":[]}"#,
    );
    let found = call(&http, json!({ "url": "https://sonar.acme.test" }), "quality.options", json!({})).await;
    assert_eq!(found, Ok(json!([])));
    let sent = http.sent();
    assert_eq!(sent[0].header_value("authorization"), Some("Bearer squ_token"));
    assert_eq!(sent[1].header_value("authorization"), Some("Basic c3F1X3Rva2VuOg=="));
}
