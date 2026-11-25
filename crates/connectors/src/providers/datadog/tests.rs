use domain::connection::ConnectionId;
use serde_json::json;

use super::*;
use crate::http::{FakeHttp, HttpResponse, Method};

const US1: &str = "https://mcp.datadoghq.com/v1/mcp";
const RESOURCE_METADATA: &str =
    r#"{"resource":"https://mcp.datadoghq.eu/v1/mcp","authorization_servers":["https://app.datadoghq.eu"]}"#;
const AUTH_METADATA: &str = r#"{"authorization_endpoint":"https://app.datadoghq.eu/oauth2/v1/authorize","token_endpoint":"https://api.datadoghq.eu/oauth2/v1/token","registration_endpoint":"https://api.datadoghq.eu/oauth2/v1/register"}"#;

fn now() -> Timestamp {
    Timestamp::from_second(1_000).unwrap_or(Timestamp::UNIX_EPOCH)
}

fn entered(fields: Value) -> Result<Loopback, String> {
    let AuthSpec::Loopback(sign_in) = Datadog.descriptor().auth else { panic!("not loopback") };
    sign_in.entered(&fields, Language::Portuguese)
}

fn listing(http: FakeHttp) -> FakeHttp {
    let answer = HttpResponse::new(200, r#"{"jsonrpc":"2.0","id":1,"result":{}}"#).with_header("mcp-session-id", "s1");
    let tools =
        r#"{"jsonrpc":"2.0","id":2,"result":{"tools":[{"name":"search_datadog_logs"},{"name":"get_datadog_trace"}]}}"#;
    http.respond(Method::Post, "/v1/mcp", answer).once(Method::Post, "/v1/mcp", 202, "").once(
        Method::Post,
        "/v1/mcp",
        200,
        tools,
    )
}

#[test]
fn a_site_is_read_from_a_name_an_address_or_nothing() {
    assert_eq!(site_of(""), Some("datadoghq.com"));
    assert_eq!(site_of(" datadoghq.eu "), Some("datadoghq.eu"));
    assert_eq!(site_of("https://app.us5.datadoghq.com/dashboard/lists"), Some("us5.datadoghq.com"));
    assert_eq!(site_of("mcp.AP1.datadoghq.com"), Some("ap1.datadoghq.com"));
    assert_eq!(site_of("ddog-gov.com"), None);
    assert_eq!(server_of("datadoghq.eu"), "https://mcp.datadoghq.eu/v1/mcp");
    assert_eq!(label_of("datadoghq.com"), "Datadog");
    assert_eq!(label_of("datadoghq.eu"), "Datadog EU1");
}

#[test]
fn the_person_picks_the_site_and_an_unknown_one_is_refused() {
    let AuthSpec::Loopback(sign_in) = Datadog.descriptor().auth else { panic!("not loopback") };
    let names: Vec<String> = sign_in.0.fields(Language::Portuguese).into_iter().map(|field| field.name).collect();
    assert_eq!(names, [SITE]);
    let filled: Vec<String> = sign_in.0.fields(Language::Portuguese).into_iter().map(|field| field.value).collect();
    assert_eq!(filled, ["datadoghq.com"]);
    assert_eq!(Datadog.descriptor().capabilities, vec![Capability::from("mcp")]);
    assert!(entered(json!({})).is_ok());
    assert!(entered(json!({ SITE: "ddog-gov.com" })).is_err_and(|told| told.contains("datadoghq.eu")));
    assert!(!Datadog.presentation(Language::Portuguese).hidden);
}

#[tokio::test]
async fn signing_in_uses_the_mcp_server_of_the_site_and_the_grant_refreshes_against_it() {
    let http = FakeHttp::new()
        .once(Method::Get, "/.well-known/oauth-protected-resource/v1/mcp", 200, RESOURCE_METADATA)
        .once(Method::Get, "/.well-known/oauth-authorization-server", 200, AUTH_METADATA)
        .once(Method::Post, "/register", 201, r#"{"client_id":"dyn1"}"#)
        .once(Method::Post, "/token", 200, r#"{"access_token":"at","refresh_token":"rt","expires_in":3600}"#)
        .once(Method::Post, "/token", 200, r#"{"access_token":"at2","expires_in":3600}"#);
    let Ok(chosen) = entered(json!({ SITE: "datadoghq.eu" })) else { panic!("refused") };
    assert_eq!(chosen.0.name(), KEY);
    let started = chosen.0.begin(&http, 4321).await;
    let Ok(started) = started else { panic!("{started:?}") };
    assert!(started.url.starts_with("https://app.datadoghq.eu/oauth2/v1/authorize?"), "{}", started.url);
    assert!(started.url.contains("resource=https%3A%2F%2Fmcp.datadoghq.eu%2Fv1%2Fmcp"), "{}", started.url);
    let grant = chosen.finish(&http, &started, &started.state, "c1", now()).await;
    let Ok(grant) = grant else { panic!("{grant:?}") };
    assert_eq!(grant.extra["resource"], "https://mcp.datadoghq.eu/v1/mcp");
    let AuthSpec::Loopback(stored) = Datadog.descriptor().auth else { panic!("not loopback") };
    let fresh = stored.0.refresh(&http, &grant, now()).await;
    assert_eq!(fresh.map(|grant| grant.access_token), Ok("at2".to_owned()));
}

#[tokio::test]
async fn the_account_points_runs_at_the_site_mcp_server_and_lists_its_tools() {
    let http = listing(FakeHttp::new());
    let creds = Credentials::new(&ConnectionId::from("d1"), "at", json!({}));
    let accounts = Datadog.accounts(&http, &creds).await;
    let Ok([account]) = accounts.as_deref() else { panic!("{accounts:?}") };
    assert_eq!(account.label, "Datadog");
    assert_eq!(
        account.settings,
        json!({ "url": US1, SITE: "datadoghq.com", "shown": { "tools": 2, "host": "datadoghq.com" } })
    );
    assert_eq!(http.sent()[0].url, US1);
    assert_eq!(http.sent()[0].header_value("authorization"), Some("Bearer at"));
    let http = listing(FakeHttp::new());
    let stored = Credentials::new(&ConnectionId::from("d1"), "at", account.settings.clone());
    let listed = Datadog.call(&http, &stored, actions::TOOLS, &json!({}), "k").await;
    assert_eq!(listed, Ok(json!([{ "name": "search_datadog_logs" }, { "name": "get_datadog_trace" }])));
    let refused = Datadog.call(&http, &stored, "tracker.issue", &json!({}), "k").await;
    assert!(refused.is_err());
}

#[tokio::test]
async fn an_access_token_is_the_bearer_and_the_site_typed_with_it_picks_the_server() {
    let [alternate] = Datadog.alternatives(Language::Portuguese).try_into().unwrap_or_else(|_| panic!("one"));
    let AuthSpec::Token(spec) = alternate.auth else { panic!("not a token") };
    let given = json!({ SITE: "app.datadoghq.eu", "token": " ddpat " });
    assert_eq!(spec.access_token(&given, Language::Portuguese), Ok("ddpat".to_owned()));
    assert_eq!(spec.settings(&given), json!({ SITE: "app.datadoghq.eu" }));
    assert!(spec.access_token(&json!({ SITE: "datadoghq.eu" }), Language::Portuguese).is_err());
    let http = FakeHttp::new()
        .respond(Method::Post, "/v1/mcp", HttpResponse::new(200, r#"{"jsonrpc":"2.0","id":1,"result":{}}"#))
        .once(Method::Post, "/v1/mcp", 202, "")
        .once(Method::Post, "/v1/mcp", 200, r#"{"jsonrpc":"2.0","id":2,"result":{"tools":[]}}"#);
    let creds = Credentials::new(&ConnectionId::from("d1"), "ddpat", spec.settings(&given));
    let accounts = Datadog.accounts(&http, &creds).await.unwrap_or_default();
    assert_eq!(accounts.first().map(|account| account.label.as_str()), Some("Datadog EU1"));
    assert_eq!(http.sent()[0].url, "https://mcp.datadoghq.eu/v1/mcp");
    assert_eq!(http.sent()[0].header_value("authorization"), Some("Bearer ddpat"));
    let wrong = Credentials::new(&ConnectionId::from("d1"), "ddpat", json!({ SITE: "ddog-gov.com" }));
    assert!(Datadog.accounts(&FakeHttp::new(), &wrong).await.is_err());
}
