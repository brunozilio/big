use domain::connection::ConnectionId;
use serde_json::json;

use super::*;
use crate::clock::FixedClock;
use crate::error::ErrorClass;
use crate::http::{FakeHttp, Method};

const GUID: &str = "1f2e3d4c-0000-4000-8000-0123456789ab";
const OTHER: &str = "aaaaaaaa-bbbb-4ccc-8ddd-eeeeeeeeeeee";

fn fixed_nonce() -> Result<Vec<u8>, ActionError> {
    Ok((0..16).collect())
}

fn creds(settings: Value) -> Credentials {
    let token = json!({ "apiId": "vera01ki-abc123", "apiKey": "vera01ks-00112233445566778899aabbccddeeff" });
    Credentials::new(&ConnectionId::from("v1"), &token.to_string(), settings)
}

async fn call(http: &FakeHttp, settings: Value, action: &str, args: Value) -> Result<Value, ActionError> {
    let clock = FixedClock::at_second(1_700_000_000);
    let api = Api::new(http, &creds(settings), &clock, fixed_nonce)?;
    let answer = run(&api, action, &args).await;
    if let Ok(value) = &answer {
        let spec = Veracode.descriptor().action(action).cloned().unwrap_or_else(|| panic!("no {action}"));
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

#[test]
fn the_provider_signs_in_with_an_api_id_a_key_and_a_region() {
    let descriptor = Veracode.descriptor();
    let AuthSpec::Token(spec) = descriptor.auth else { panic!("not a token") };
    let names: Vec<(&str, bool, bool)> =
        spec.fields.iter().map(|field| (field.name.as_str(), field.secret, field.setting)).collect();
    assert_eq!(names, [("region", false, true), ("apiId", false, false), ("apiKey", true, false)]);
    assert_eq!(spec.fields[0].value, brand::connectors::VERACODE_REGION);
    assert_eq!(spec.form, TokenForm::Json);
    assert_eq!(descriptor.capabilities, vec![Capability::from(contracts::SECURITY)]);
    assert_eq!(Veracode.presentation(Language::Portuguese).group, "security");
    assert!(spec.fields.iter().all(|field| field.help.is_empty()));
}

#[test]
fn regions_name_their_api_host() {
    assert_eq!(region_of("eu").map(|r| r.1), Some("api.veracode.eu"));
    assert_eq!(region_of(" https://api.veracode.us/ ").map(|r| r.1), Some("api.veracode.us"));
    assert_eq!(region_of("").map(|r| r.0), Some(brand::connectors::VERACODE_REGION));
    assert_eq!(region_of("mars"), None);
    assert!(is_guid(GUID));
    assert!(!is_guid("My App"));
}

#[tokio::test]
async fn every_request_carries_the_veracode_signature_for_its_host_and_path() {
    let http = FakeHttp::new().once(Method::Get, "/appsec/v1/applications", 200, &applications_page(&[], 1));
    let found = call(&http, json!({ "region": "eu" }), actions::PROJECTS, json!({})).await;
    assert_eq!(found, Ok(json!([])));
    let sent = http.sent();
    assert_eq!(sent[0].url, "https://api.veracode.eu/appsec/v1/applications?page=0&size=100");
    let keys = Keys { api_id: "abc123".to_owned(), api_key: "00112233445566778899aabbccddeeff".to_owned() };
    let request = sign::Request {
        method: "GET",
        host: "api.veracode.eu",
        path_and_query: "/appsec/v1/applications?page=0&size=100",
    };
    let nonce = fixed_nonce().unwrap_or_default();
    let expected = sign::header(&keys, request, 1_700_000_000_000, &nonce);
    assert_eq!(sent[0].header_value("authorization"), expected.as_deref());
    let told = sent[0].header_value("authorization").unwrap_or_default();
    assert!(
        told.starts_with(
            "VERACODE-HMAC-SHA-256 id=abc123,ts=1700000000000,nonce=000102030405060708090a0b0c0d0e0f,sig="
        )
    );
}

#[tokio::test]
async fn projects_are_listed_across_pages_and_filtered_by_the_query() {
    let http = FakeHttp::new().once(Method::Get, "page=0", 200, &applications_page(&[(GUID, "Shop API")], 2)).once(
        Method::Get,
        "page=1",
        200,
        &applications_page(&[(OTHER, "Back office")], 2),
    );
    let found = call(&http, json!({}), actions::PROJECTS, json!({})).await;
    assert_eq!(
        found,
        Ok(json!([
            { "id": GUID, "key": GUID, "name": "Shop API" },
            { "id": OTHER, "key": OTHER, "name": "Back office" },
        ]))
    );
    assert_eq!(http.sent()[0].url, "https://api.veracode.com/appsec/v1/applications?page=0&size=100");
    let http = FakeHttp::new().once(Method::Get, "name=shop", 200, &applications_page(&[(GUID, "Shop API")], 1));
    let found = call(&http, json!({}), actions::PROJECTS, json!({ "query": "shop" })).await;
    assert_eq!(found.map(|listed| listed[0]["name"].clone()), Ok(json!("Shop API")));
}

#[tokio::test]
async fn findings_are_normalized_whatever_their_scan() {
    let http = every_scan(FakeHttp::new(), &[static_finding(7, 4)], &[sca_finding(9, 3)]);
    let found = call(&http, json!({ "project": GUID }), actions::FINDINGS, json!({})).await.unwrap_or_default();
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
    let found = call(&http, json!({ "project": GUID }), actions::FINDINGS, json!({ "limit": 3 })).await;
    let ids = |found: &Value| found.as_array().into_iter().flatten().map(|f| text_of(f, "/id")).collect::<Vec<_>>();
    assert_eq!(found.as_ref().map(ids), Ok(vec!["9".to_owned(), "10".to_owned(), "11".to_owned()]));

    let statics = [static_finding(20, 2), static_finding(11, 4)];
    let http = FakeHttp::new().once(Method::Get, "scan_type=STATIC", 200, &findings_page(&statics));
    let args = json!({ "kinds": ["static"], "minSeverity": 3, "policy": true, "open": false });
    let found = call(&http, json!({ "project": GUID }), actions::FINDINGS, args).await;
    assert_eq!(found.as_ref().map(ids), Ok(vec!["11".to_owned()]));
    assert_eq!(http.sent().len(), 1);
    assert!(http.sent()[0].url.ends_with("scan_type=STATIC&violates_policy=TRUE&severity_gte=3"));

    let mut closed = static_finding(3, 5);
    closed["finding_status"]["status"] = json!("CLOSED");
    let http = FakeHttp::new().once(Method::Get, "scan_type=STATIC", 200, &findings_page(&[closed]));
    let args = json!({ "kinds": ["static"], "open": false });
    let found = call(&http, json!({ "project": GUID }), actions::FINDINGS, args).await;
    assert_eq!(found.map(|listed| listed[0]["status"].clone()), Ok(json!("closed")));
}

#[tokio::test]
async fn a_bad_kind_or_a_missing_project_stops_before_any_request() {
    let http = FakeHttp::new();
    let found = call(&http, json!({ "project": GUID }), actions::FINDINGS, json!({ "kinds": ["fuzz"] })).await;
    assert_eq!(found.map_err(|error| error.class), Err(ErrorClass::Permanent));
    let found = call(&http, json!({}), actions::FINDINGS, json!({})).await;
    assert_eq!(found.map_err(|error| error.blocker().map(str::to_owned)), Err(Some(NO_PROJECT.to_owned())));
    assert!(http.sent().is_empty());
}

#[tokio::test]
async fn a_project_name_is_resolved_to_its_guid() {
    let listed = applications_page(&[(OTHER, "Shop API v2"), (GUID, "Shop API")], 1);
    let http = every_scan(FakeHttp::new().once(Method::Get, "/appsec/v1/applications", 200, &listed), &[], &[]);
    let found = call(&http, json!({ "project": OTHER }), actions::FINDINGS, json!({ "project": "shop api" })).await;
    assert_eq!(found, Ok(json!([])));
    let sent = http.sent();
    assert_eq!(sent[0].url, "https://api.veracode.com/appsec/v1/applications?page=0&size=100&name=shop+api");
    assert!(sent[1].url.contains(&format!("/applications/{GUID}/findings")));
    let http = FakeHttp::new().once(Method::Get, "/appsec/v1/applications", 200, &applications_page(&[], 1));
    let found = call(&http, json!({}), actions::FINDINGS, json!({ "project": "Nope" })).await;
    assert_eq!(found.map_err(|error| error.class), Err(ErrorClass::Permanent));
}

#[tokio::test]
async fn refused_credentials_are_an_auth_error() {
    let http = FakeHttp::new().once(Method::Get, "/appsec/v1/applications", 401, "{\"message\":\"no\"}");
    let found = call(&http, json!({}), actions::PROJECTS, json!({})).await;
    assert_eq!(found.map_err(|error| error.class), Err(ErrorClass::Auth));
    let http = FakeHttp::new();
    let bad = Credentials::new(&ConnectionId::from("v1"), "not json", json!({}));
    let found = Veracode.call(&http, &bad, actions::PROJECTS, &json!({}), "k").await;
    assert_eq!(found.map_err(|error| error.class), Err(ErrorClass::Auth));
    let found = Veracode.call(&http, &creds(json!({ "region": "mars" })), actions::PROJECTS, &json!({}), "k").await;
    assert_eq!(found.map_err(|error| error.class), Err(ErrorClass::Permanent));
}

#[tokio::test]
async fn the_account_shows_the_applications_and_picks_the_only_one() {
    let http =
        FakeHttp::new().once(Method::Get, "/appsec/v1/applications", 200, &applications_page(&[(GUID, "Shop API")], 1));
    let listed = Veracode.accounts(&http, &creds(json!({ "region": "us" }))).await;
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

#[test]
fn descriptions_lose_their_markup_and_stay_short() {
    assert_eq!(findings::plain("<p>a&nbsp;&quot;b&quot;</p>\n<br/>c &#39;d&#39;"), "a \"b\" c 'd'");
    assert_eq!(findings::plain(&"x".repeat(3000)).len(), 2000);
}
