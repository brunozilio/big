use domain::Language;
use serde_json::{Value, json};

use super::{creds, loaded, now};
use crate::descriptor::AuthSpec;
use crate::http::{FakeHttp, HttpResponse, Method};
use crate::oauth::Loopback;
use crate::provider::Provider;

const US1: &str = "https://mcp.datadoghq.com/v1/mcp";
const RESOURCE_METADATA: &str =
    r#"{"resource":"https://mcp.datadoghq.eu/v1/mcp","authorization_servers":["https://app.datadoghq.eu"]}"#;
const AUTH_METADATA: &str = r#"{"authorization_endpoint":"https://app.datadoghq.eu/oauth2/v1/authorize","token_endpoint":"https://api.datadoghq.eu/oauth2/v1/token","registration_endpoint":"https://api.datadoghq.eu/oauth2/v1/register"}"#;

fn sign_in() -> Loopback {
    let AuthSpec::Loopback(sign_in) = loaded("datadog").descriptor().auth else { panic!("not loopback") };
    sign_in
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
fn the_person_picks_the_site_and_an_unknown_one_is_refused() {
    let fields = sign_in().0.fields(Language::Portuguese);
    let names: Vec<&str> = fields.iter().map(|field| field.name.as_str()).collect();
    assert_eq!(names, ["site"]);
    assert_eq!(fields[0].value, "datadoghq.com");
    assert!(fields[0].setting);
    assert!(sign_in().entered(&json!({}), Language::Portuguese).is_ok());
    let refused = sign_in().entered(&json!({ "site": "ddog-gov.com" }), Language::Portuguese);
    assert!(refused.is_err_and(|told| told.contains("datadoghq.eu") && told.starts_with("Use um dos sites")));
    let provider = loaded("datadog");
    assert_eq!(provider.descriptor().capabilities, vec![domain::connection::Capability::from("mcp")]);
    assert!(!provider.presentation(Language::Portuguese).hidden);
    assert_eq!(provider.presentation(Language::English).button, "Continue on Datadog");
}

#[tokio::test]
async fn signing_in_uses_the_mcp_server_of_the_site_and_the_grant_refreshes_against_it() {
    let http = FakeHttp::new()
        .once(Method::Get, "/.well-known/oauth-protected-resource/v1/mcp", 200, RESOURCE_METADATA)
        .once(Method::Get, "/.well-known/oauth-authorization-server", 200, AUTH_METADATA)
        .once(Method::Post, "/register", 201, r#"{"client_id":"dyn1"}"#)
        .once(Method::Post, "/token", 200, r#"{"access_token":"at","refresh_token":"rt","expires_in":3600}"#)
        .once(Method::Post, "/token", 200, r#"{"access_token":"at2","expires_in":3600}"#);
    let Ok(chosen) = sign_in().entered(&json!({ "site": "app.datadoghq.eu" }), Language::Portuguese) else {
        panic!("refused")
    };
    assert_eq!(chosen.0.name(), "datadog");
    let started = chosen.0.begin(&http, 4321).await;
    let Ok(started) = started else { panic!("{started:?}") };
    assert!(started.url.starts_with("https://app.datadoghq.eu/oauth2/v1/authorize?"), "{}", started.url);
    assert!(started.url.contains("resource=https%3A%2F%2Fmcp.datadoghq.eu%2Fv1%2Fmcp"), "{}", started.url);
    let grant = chosen.finish(&http, &started, &started.state, "c1", now()).await;
    let Ok(grant) = grant else { panic!("{grant:?}") };
    assert_eq!(grant.extra["resource"], "https://mcp.datadoghq.eu/v1/mcp");
    assert_eq!(grant.refresh_token.as_deref(), Some("rt"));
    let fresh = sign_in().0.refresh(&http, &grant, now()).await;
    assert_eq!(fresh.map(|grant| grant.access_token), Ok("at2".to_owned()));
}

#[tokio::test]
async fn the_account_points_runs_at_the_site_mcp_server_and_lists_its_tools() {
    let provider = loaded("datadog");
    let http = listing(FakeHttp::new());
    let accounts = provider.accounts(&http, &creds("at", json!({}))).await;
    let Ok([account]) = accounts.as_deref() else { panic!("{accounts:?}") };
    assert_eq!(account.label, "Datadog");
    assert_eq!(
        account.settings,
        json!({ "url": US1, "site": "datadoghq.com", "shown": { "tools": 2, "host": "datadoghq.com" } })
    );
    assert_eq!(http.sent()[0].url, US1);
    assert_eq!(http.sent()[0].header_value("authorization"), Some("Bearer at"));
    let tools =
        r#"{"jsonrpc":"2.0","id":3,"result":{"tools":[{"name":"search_datadog_logs"},{"name":"get_datadog_trace"}]}}"#;
    let http = FakeHttp::new().once(Method::Post, "/v1/mcp", 200, tools);
    let stored = creds("at", account.settings.clone());
    let listed = provider.call(&http, &stored, "mcp.tools", &json!({}), "k").await;
    assert_eq!(listed, Ok(json!([{ "name": "search_datadog_logs" }, { "name": "get_datadog_trace" }])));
    assert_eq!(http.sent().len(), 1);
    assert_eq!(http.sent()[0].header_value("mcp-session-id"), Some("s1"));
}

#[tokio::test]
async fn an_access_token_is_the_bearer_and_the_site_typed_with_it_picks_the_server() {
    let provider = loaded("datadog");
    let [alternate] = provider.alternatives(Language::Portuguese).try_into().unwrap_or_else(|_| panic!("one"));
    assert_eq!(alternate.key, "access_token");
    let AuthSpec::Token(spec) = alternate.auth else { panic!("not a token") };
    let given = json!({ "site": "app.datadoghq.eu", "token": " ddpat " });
    assert_eq!(spec.access_token(&given, Language::Portuguese), Ok("ddpat".to_owned()));
    assert_eq!(spec.settings(&given), json!({ "site": "app.datadoghq.eu" }));
    let http = FakeHttp::new()
        .respond(Method::Post, "/v1/mcp", HttpResponse::new(200, r#"{"jsonrpc":"2.0","id":1,"result":{}}"#))
        .once(Method::Post, "/v1/mcp", 202, "")
        .once(Method::Post, "/v1/mcp", 200, r#"{"jsonrpc":"2.0","id":2,"result":{"tools":[]}}"#);
    let accounts = provider.accounts(&http, &creds("ddpat", spec.settings(&given))).await.unwrap_or_default();
    assert_eq!(accounts.first().map(|account| account.label.as_str()), Some("Datadog EU1"));
    assert_eq!(http.sent()[0].url, "https://mcp.datadoghq.eu/v1/mcp");
    let wrong = provider.accounts(&FakeHttp::new(), &creds("ddpat", json!({ "site": "ddog-gov.com" }))).await;
    assert!(wrong.is_err());
    let _: Value = json!(null);
}
