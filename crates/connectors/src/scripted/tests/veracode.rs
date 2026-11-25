use std::sync::Arc;

use domain::Language;
use domain::connection::Capability;
use serde_json::{Value, json};

use super::{creds as creds_with, loaded};
use crate::descriptor::{AuthSpec, TokenForm};
use crate::error::{ActionError, ErrorClass};
use crate::http::{FakeHttp, Method};
use crate::provider::{AccountOption, Credentials, Provider};
use crate::scripted::{Answers, Local, ScriptedProvider};

const GUID: &str = "1f2e3d4c-0000-4000-8000-0123456789ab";
const OTHER: &str = "aaaaaaaa-bbbb-4ccc-8ddd-eeeeeeeeeeee";
const NO_PROJECT: &str = "Choose the project.";
const NONCE: &str = "000102030405060708090a0b0c0d0e0f";

#[derive(Debug)]
struct Fixed {
    millis: i64,
}

impl Answers for Fixed {
    fn now(&self, function: &str, arguments: &Value) -> Option<Result<Value, String>> {
        match function {
            "clock.millis" => Some(Ok(json!(self.millis))),
            "crypto.random" => Some(Ok(json!(NONCE))),
            _ => Local.now(function, arguments),
        }
    }
}

fn veracode_at(millis: i64) -> ScriptedProvider {
    loaded("veracode").with_answers(Arc::new(Fixed { millis }))
}

fn veracode() -> ScriptedProvider {
    veracode_at(1_700_000_000_000)
}

fn creds(settings: Value) -> Credentials {
    let token = json!({ "apiId": "vera01ki-abc123", "apiKey": "vera01ks-00112233445566778899aabbccddeeff" });
    creds_with(&token.to_string(), settings)
}

async fn call(http: &FakeHttp, settings: Value, action: &str, args: Value) -> Result<Value, ActionError> {
    let provider = veracode();
    let answer = provider.call(http, &creds(settings), action, &args, "k").await;
    if let Ok(value) = &answer {
        let spec = provider.descriptor().action(action).cloned().unwrap_or_else(|| panic!("no {action}"));
        assert_eq!(crate::descriptor::conforms(&spec.output_schema, value), Ok(()), "{action}: {value}");
    }
    answer
}

fn applications_page(listed: &[(&str, &str)], total: u64) -> String {
    let listed: Vec<Value> =
        listed.iter().map(|(guid, name)| json!({ "guid": guid, "profile": { "name": name } })).collect();
    json!({ "_embedded": { "applications": listed }, "page": { "total_pages": total } }).to_string()
}

fn findings_page(findings: &[Value]) -> String {
    json!({ "_embedded": { "findings": findings }, "page": { "total_pages": 1 } }).to_string()
}

fn static_finding(id: u64, severity: u64) -> Value {
    json!({
        "issue_id": id,
        "scan_type": "STATIC",
        "description": "<span>Uses &lt;b&gt; tags &amp; more.</span> <a href=\"x\">See</a>",
        "violates_policy": true,
        "finding_status": { "status": "OPEN", "resolution": "UNRESOLVED", "resolution_status": "NONE", "new": true, "first_found_date": "2024-01-02T03:04:05.000Z" },
        "finding_details": {
            "severity": severity, "cwe": { "id": 89, "name": "SQL Injection" },
            "file_path": "src/db/query.java", "file_name": "query.java", "file_line_number": 42, "module": "app.war",
        },
    })
}

fn sca_finding(id: u64, severity: u64) -> Value {
    json!({
        "issue_id": id,
        "scan_type": "SCA",
        "description": "Prototype pollution.",
        "violates_policy": false,
        "finding_status": { "status": "OPEN", "resolution_status": "NONE", "new": false },
        "finding_details": {
            "severity": severity, "component_filename": "lodash-4.17.15.js", "version": "4.17.15",
            "cwe": { "id": 1321, "name": "Prototype Pollution" },
            "cve": { "name": "CVE-2020-8203", "cvss": 5.8, "cvss3": { "score": 7.4 } },
        },
    })
}

fn every_scan(http: FakeHttp, statics: &[Value], scas: &[Value]) -> FakeHttp {
    http.once(Method::Get, "scan_type=STATIC", 200, &findings_page(statics))
        .once(Method::Get, "scan_type=SCA", 200, &findings_page(scas))
        .once(Method::Get, "scan_type=DYNAMIC", 200, &findings_page(&[]))
        .once(Method::Get, "scan_type=MANUAL", 200, &findings_page(&[]))
}

fn ids(found: &Value) -> Vec<String> {
    found.as_array().into_iter().flatten().map(|f| f["id"].as_str().unwrap_or_default().to_owned()).collect()
}

#[test]
fn the_provider_signs_in_with_an_api_id_a_key_and_a_region() {
    let provider = loaded("veracode");
    let descriptor = provider.descriptor();
    let AuthSpec::Token(spec) = descriptor.auth else { panic!("not a token") };
    let names: Vec<(&str, bool, bool)> =
        spec.fields.iter().map(|field| (field.name.as_str(), field.secret, field.setting)).collect();
    assert_eq!(names, [("region", false, true), ("apiId", false, false), ("apiKey", true, false)]);
    assert_eq!(spec.fields[0].value, "com");
    assert_eq!(spec.form, TokenForm::Json);
    assert_eq!(descriptor.capabilities, vec![Capability::from("security")]);
    assert_eq!(provider.presentation(Language::Portuguese).group, "security");
    assert!(spec.fields.iter().all(|field| field.help.is_empty()));
    let bindings = provider.presentation(Language::Portuguese).bindings;
    let declared: Vec<(&str, &str, &str)> = bindings
        .iter()
        .map(|binding| (binding.capability.as_str(), binding.scope.as_str(), binding.label.as_str()))
        .collect();
    assert_eq!(declared, [("security", "repository", "Veracode")]);
}

#[tokio::test]
async fn the_status_says_when_a_scan_finished_after_the_given_moment() {
    let application = json!({
        "guid": GUID, "last_completed_scan_date": "2026-11-20T10:00:00.000Z",
        "scans": [
            { "scan_type": "STATIC", "status": "PUBLISHED", "modified_date": "2026-11-24T09:30:00.000Z" },
            { "scan_type": "SCA", "status": "SCAN_IN_PROGRESS", "modified_date": "2026-11-25T08:00:00.000Z" },
        ],
    })
    .to_string();
    let http = FakeHttp::new().once(Method::Get, GUID, 200, &application);
    let found = call(&http, json!({}), "security.status", json!({ "project": GUID })).await;
    assert_eq!(found, Ok(json!({ "status": "done", "scannedAt": "2026-11-24T09:30:00.000Z", "running": true })));
    assert!(http.sent()[0].url.ends_with(&format!("/appsec/v1/applications/{GUID}")));
    let http = FakeHttp::new().once(Method::Get, GUID, 200, &application);
    let since = json!({ "project": GUID, "since": "2026-11-24T09:30:00.000Z" });
    let found = call(&http, json!({}), "security.status", since).await;
    assert_eq!(found.map(|found| found["status"].clone()), Ok(json!("pending")));
}

#[tokio::test]
async fn the_options_to_bind_a_repository_are_its_applications_by_name() {
    let http = FakeHttp::new().once(Method::Get, "page=0", 200, &applications_page(&[(GUID, "Shop API")], 1));
    let found = call(&http, json!({}), "security.options", json!({})).await;
    assert_eq!(found, Ok(json!([{ "value": GUID, "label": format!("Shop API · {GUID}") }])));
}

#[tokio::test]
async fn the_signature_follows_the_veracode_key_chain() {
    let key = "0123456789abcdef".repeat(8);
    let token = json!({ "apiId": "3ddaeeb10ca690df3fee5e3bd1c329fa", "apiKey": key });
    let http = FakeHttp::new().once(Method::Get, "/appsec/v1/applications", 200, &applications_page(&[], 1));
    let found = veracode_at(1_700_000_000_123)
        .call(&http, &creds_with(&token.to_string(), json!({})), "security.projects", &json!({}), "k")
        .await;
    assert_eq!(found, Ok(json!([])));
    assert_eq!(
        http.sent()[0].header_value("authorization"),
        Some(
            "VERACODE-HMAC-SHA-256 id=3ddaeeb10ca690df3fee5e3bd1c329fa,ts=1700000000123,nonce=000102030405060708090a0b0c0d0e0f,sig=70efdd004ee88f27c06d7e38b108c0b1a71f2629a76a2976751bf85d142e4ad3"
        )
    );
}

#[tokio::test]
async fn typed_keys_lose_their_prefix_and_a_key_that_is_not_hex_cannot_sign() {
    let bad = json!({ "apiId": "a", "apiKey": "xyz" });
    let refused = veracode()
        .call(&FakeHttp::new(), &creds_with(&bad.to_string(), json!({})), "security.projects", &json!({}), "k")
        .await;
    assert!(refused.is_err_and(|error| error.class == ErrorClass::Auth && error.message.contains("not valid hex")));
    for empty in [json!({ "apiId": "", "apiKey": "0f" }), json!("not json")] {
        let text = empty.as_str().map_or_else(|| empty.to_string(), str::to_owned);
        let refused = veracode()
            .call(&FakeHttp::new(), &creds_with(&text, json!({})), "security.projects", &json!({}), "k")
            .await;
        assert_eq!(refused.map_err(|error| error.class), Err(ErrorClass::Auth));
    }
}

#[tokio::test]
async fn regions_name_their_api_host_and_a_guid_skips_the_lookup() {
    for (region, host) in
        [("eu", "api.veracode.eu"), (" https://api.veracode.us/ ", "api.veracode.us"), ("", "api.veracode.com")]
    {
        let http = FakeHttp::new().once(Method::Get, "/appsec/v1/applications", 200, &applications_page(&[], 1));
        assert_eq!(call(&http, json!({ "region": region }), "security.projects", json!({})).await, Ok(json!([])));
        assert!(http.sent()[0].url.starts_with(&format!("https://{host}/")), "{region}");
    }
    let mars = call(&FakeHttp::new(), json!({ "region": "mars" }), "security.projects", json!({})).await;
    assert_eq!(mars.map_err(|error| error.class), Err(ErrorClass::Permanent));
    let http = every_scan(FakeHttp::new(), &[], &[]);
    assert_eq!(call(&http, json!({ "project": GUID }), "security.findings", json!({})).await, Ok(json!([])));
    assert_eq!(http.sent().len(), 4);
}

#[tokio::test]
async fn every_request_carries_the_veracode_signature_for_its_host_and_path() {
    let http = FakeHttp::new().once(Method::Get, "/appsec/v1/applications", 200, &applications_page(&[], 1));
    let found = call(&http, json!({ "region": "eu" }), "security.projects", json!({})).await;
    assert_eq!(found, Ok(json!([])));
    let sent = http.sent();
    assert_eq!(sent[0].url, "https://api.veracode.eu/appsec/v1/applications?page=0&size=100");
    let told = sent[0].header_value("authorization").unwrap_or_default();
    let prefix = "VERACODE-HMAC-SHA-256 id=abc123,ts=1700000000000,nonce=000102030405060708090a0b0c0d0e0f,sig=";
    assert!(told.starts_with(prefix), "{told}");
    let sig = told.trim_start_matches(prefix);
    assert!(sig.len() == 64 && sig.chars().all(|c| c.is_ascii_hexdigit()), "{sig}");
}

#[tokio::test]
async fn projects_are_listed_across_pages_and_filtered_by_the_query() {
    let http = FakeHttp::new().once(Method::Get, "page=0", 200, &applications_page(&[(GUID, "Shop API")], 2)).once(
        Method::Get,
        "page=1",
        200,
        &applications_page(&[(OTHER, "Back office")], 2),
    );
    let found = call(&http, json!({}), "security.projects", json!({})).await;
    assert_eq!(
        found,
        Ok(json!([
            { "id": GUID, "key": GUID, "name": "Shop API" },
            { "id": OTHER, "key": OTHER, "name": "Back office" },
        ]))
    );
    assert_eq!(http.sent()[0].url, "https://api.veracode.com/appsec/v1/applications?page=0&size=100");
    let http = FakeHttp::new().once(Method::Get, "name=shop", 200, &applications_page(&[(GUID, "Shop API")], 1));
    let found = call(&http, json!({}), "security.projects", json!({ "query": "shop" })).await;
    assert_eq!(found.map(|listed| listed[0]["name"].clone()), Ok(json!("Shop API")));
}

#[tokio::test]
async fn findings_are_normalized_whatever_their_scan() {
    let http = every_scan(FakeHttp::new(), &[static_finding(7, 4)], &[sca_finding(9, 3)]);
    let found = call(&http, json!({ "project": GUID }), "security.findings", json!({})).await.unwrap_or_default();
    assert_eq!(
        found[0],
        json!({
            "id": "7", "title": "SQL Injection", "severity": 4, "kind": "static", "status": "open", "new": true,
            "policy": true, "cwe": { "id": "89", "name": "SQL Injection" }, "cve": null,
            "file": "src/db/query.java", "line": 42, "module": "app.war", "component": null, "version": null,
            "description": "Uses <b> tags & more. See", "found": "2024-01-02T03:04:05.000Z", "url": null,
        })
    );
    assert_eq!(
        found[1],
        json!({
            "id": "9", "title": "CVE-2020-8203 in lodash-4.17.15.js", "severity": 3, "kind": "dependency",
            "status": "open", "new": false, "policy": false, "cwe": { "id": "1321", "name": "Prototype Pollution" },
            "cve": { "id": "CVE-2020-8203", "cvss": 7.4 }, "file": null, "line": null, "module": null,
            "component": "lodash-4.17.15.js", "version": "4.17.15", "description": "Prototype pollution.",
            "found": null, "url": null,
        })
    );
    let urls: Vec<String> = http.sent().iter().map(|request| request.url.clone()).collect();
    assert_eq!(
        urls[0],
        format!("https://api.veracode.com/appsec/v2/applications/{GUID}/findings?size=500&page=0&scan_type=STATIC")
    );
    assert_eq!(urls.len(), 4);
}

#[tokio::test]
async fn findings_are_filtered_sorted_and_limited() {
    let mut closed = static_finding(3, 5);
    closed["finding_status"]["status"] = json!("CLOSED");
    let mut approved = static_finding(4, 5);
    approved["finding_status"]["resolution_status"] = json!("APPROVED");
    let statics = [static_finding(20, 2), static_finding(11, 4), closed, approved, static_finding(10, 4)];
    let http = every_scan(FakeHttp::new(), &statics, &[sca_finding(9, 5)]);
    let found = call(&http, json!({ "project": GUID }), "security.findings", json!({ "limit": 3 })).await;
    assert_eq!(found.as_ref().map(ids), Ok(vec!["9".to_owned(), "10".to_owned(), "11".to_owned()]));

    let statics = [static_finding(20, 2), static_finding(11, 4)];
    let http = FakeHttp::new().once(Method::Get, "scan_type=STATIC", 200, &findings_page(&statics));
    let args = json!({ "kinds": ["static"], "minSeverity": 3, "policy": true, "open": false });
    let found = call(&http, json!({ "project": GUID }), "security.findings", args).await;
    assert_eq!(found.as_ref().map(ids), Ok(vec!["11".to_owned()]));
    assert_eq!(http.sent().len(), 1);
    assert!(http.sent()[0].url.ends_with("scan_type=STATIC&violates_policy=TRUE&severity_gte=3"));

    let mut closed = static_finding(3, 5);
    closed["finding_status"]["status"] = json!("CLOSED");
    let http = FakeHttp::new().once(Method::Get, "scan_type=STATIC", 200, &findings_page(&[closed]));
    let args = json!({ "kinds": ["static"], "open": false });
    let found = call(&http, json!({ "project": GUID }), "security.findings", args).await;
    assert_eq!(found.map(|listed| listed[0]["status"].clone()), Ok(json!("closed")));
}

#[tokio::test]
async fn a_bad_kind_or_a_missing_project_stops_before_any_request() {
    let http = FakeHttp::new();
    let found = call(&http, json!({ "project": GUID }), "security.findings", json!({ "kinds": ["fuzz"] })).await;
    assert_eq!(found.map_err(|error| error.class), Err(ErrorClass::Permanent));
    let found = call(&http, json!({}), "security.findings", json!({})).await;
    assert_eq!(found.map_err(|error| error.blocker().map(str::to_owned)), Err(Some(NO_PROJECT.to_owned())));
    assert!(http.sent().is_empty());
}

#[tokio::test]
async fn a_project_name_is_resolved_to_its_guid() {
    let listed = applications_page(&[(OTHER, "Shop API v2"), (GUID, "Shop API")], 1);
    let http = every_scan(FakeHttp::new().once(Method::Get, "/appsec/v1/applications", 200, &listed), &[], &[]);
    let found = call(&http, json!({ "project": OTHER }), "security.findings", json!({ "project": "shop api" })).await;
    assert_eq!(found, Ok(json!([])));
    let sent = http.sent();
    assert_eq!(sent[0].url, "https://api.veracode.com/appsec/v1/applications?page=0&size=100&name=shop+api");
    assert!(sent[1].url.contains(&format!("/applications/{GUID}/findings")));
    let http = FakeHttp::new().once(Method::Get, "/appsec/v1/applications", 200, &applications_page(&[], 1));
    let found = call(&http, json!({}), "security.findings", json!({ "project": "Nope" })).await;
    assert_eq!(found.map_err(|error| error.class), Err(ErrorClass::Permanent));
}

#[tokio::test]
async fn refused_credentials_are_an_auth_error() {
    let http = FakeHttp::new().once(Method::Get, "/appsec/v1/applications", 401, "{\"message\":\"no\"}");
    let found = call(&http, json!({}), "security.projects", json!({})).await;
    assert_eq!(found.map_err(|error| error.class), Err(ErrorClass::Auth));
    let http = FakeHttp::new();
    let found = veracode().call(&http, &creds_with("not json", json!({})), "security.projects", &json!({}), "k").await;
    assert_eq!(found.map_err(|error| error.class), Err(ErrorClass::Auth));
    let found = veracode().call(&http, &creds(json!({ "region": "mars" })), "security.projects", &json!({}), "k").await;
    assert_eq!(found.map_err(|error| error.class), Err(ErrorClass::Permanent));
}

#[tokio::test]
async fn the_account_shows_the_applications_and_picks_the_only_one() {
    let http =
        FakeHttp::new().once(Method::Get, "/appsec/v1/applications", 200, &applications_page(&[(GUID, "Shop API")], 1));
    let listed = loaded("veracode").accounts(&http, &creds(json!({ "region": "us" }))).await;
    let expected = AccountOption {
        id: "veracode-us".to_owned(),
        label: "Veracode FedRAMP".to_owned(),
        settings: json!({ "region": "us", "project": GUID, "shown": { "projects": ["Shop API"] } }),
    };
    assert_eq!(listed, Ok(vec![expected]));
    assert!(http.sent()[0].url.starts_with("https://api.veracode.us/"));
    let told = http.sent()[0].header_value("authorization").unwrap_or_default().to_owned();
    assert!(told.contains(",nonce=") && told.len() > 100);
}

#[tokio::test]
async fn descriptions_lose_their_markup_and_stay_short() {
    let mut finding = static_finding(1, 3);
    finding["description"] = json!("<p>a&nbsp;&quot;b&quot;</p>\n<br/>c &#39;d&#39;");
    let mut long = static_finding(2, 3);
    long["description"] = json!("x".repeat(3000));
    let http = FakeHttp::new().once(Method::Get, "scan_type=STATIC", 200, &findings_page(&[finding, long]));
    let found = call(&http, json!({ "project": GUID }), "security.findings", json!({ "kinds": ["static"] })).await;
    let found = found.unwrap_or_default();
    assert_eq!(found[0]["description"], "a \"b\" c 'd'");
    assert_eq!(found[1]["description"].as_str().map(str::len), Some(2000));
}
