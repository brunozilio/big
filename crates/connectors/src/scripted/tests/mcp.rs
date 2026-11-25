use domain::Language;
use domain::connection::Capability;
use serde_json::{Value, json};

use super::{creds, loaded, now};
use crate::descriptor::AuthSpec;
use crate::error::ErrorClass;
use crate::http::{FakeHttp, HttpResponse, Method};
use crate::oauth::{Grant, Loopback};
use crate::provider::Provider;

const SERVER: &str = "https://tools.test/mcp";
const RESOURCE_METADATA: &str =
    r#"{"resource":"https://tools.test/mcp","authorization_servers":["https://auth.tools.test"]}"#;
const AUTH_METADATA: &str = r#"{"authorization_endpoint":"https://auth.tools.test/authorize","token_endpoint":"https://auth.tools.test/token","registration_endpoint":"https://auth.tools.test/register"}"#;

fn initialized(http: FakeHttp) -> FakeHttp {
    let answer = HttpResponse::new(200, r#"{"jsonrpc":"2.0","id":1,"result":{}}"#).with_header("mcp-session-id", "s1");
    http.respond(Method::Post, "/mcp", answer).once(Method::Post, "/mcp", 202, "")
}

fn listing(http: FakeHttp) -> FakeHttp {
    let tools = r#"{"jsonrpc":"2.0","id":2,"result":{"tools":[{"name":"search"},{"name":"create_issue"}]}}"#;
    initialized(http).once(Method::Post, "/mcp", 200, tools)
}

fn named(token: &str) -> crate::provider::Credentials {
    creds(token, json!({ "url": SERVER, "label": "Linear" }))
}

fn generic() -> Loopback {
    let AuthSpec::Loopback(generic) = loaded("mcp").descriptor().auth else { panic!("not loopback") };
    generic
}

fn entered(fields: Value) -> Result<Loopback, String> {
    generic().entered(&fields, Language::Portuguese)
}

#[test]
fn the_person_enters_the_server_address_and_a_name_and_only_safe_addresses_pass() {
    let names: Vec<String> = generic().0.fields(Language::Portuguese).into_iter().map(|field| field.name).collect();
    assert_eq!(names, ["url", "label"]);
    assert_eq!(loaded("mcp").descriptor().key.0, "mcp");
    assert_eq!(loaded("mcp").descriptor().capabilities, vec![Capability::from("mcp")]);
    assert!(loaded("mcp").presentation(Language::Portuguese).hidden);
    assert!(entered(json!({})).is_err());
    assert!(entered(json!({ "url": "http://tools.test/mcp" })).is_err());
    assert!(entered(json!({ "url": "http://localhost:3000/mcp" })).is_ok());
    let Ok(chosen) = entered(json!({ "url": format!(" {SERVER} ") })) else { panic!("refused") };
    assert_eq!(chosen.0.name(), SERVER);
}

#[tokio::test]
async fn a_server_that_answers_without_a_token_connects_without_sign_in() {
    let http = initialized(FakeHttp::new());
    let Ok(chosen) = entered(json!({ "url": SERVER })) else { panic!("refused") };
    let grant = chosen.0.without_sign_in(&http).await;
    assert_eq!(grant, Ok(Some(Grant::bearer(""))));
    assert!(http.sent().iter().all(|sent| sent.header_value("authorization").is_none()));
}

#[tokio::test]
async fn a_server_that_refuses_without_a_token_is_signed_into_after_discovery() {
    let http = FakeHttp::new()
        .once(Method::Post, "/mcp", 401, "")
        .once(Method::Get, "/.well-known/oauth-protected-resource/mcp", 200, RESOURCE_METADATA)
        .once(Method::Get, "/.well-known/oauth-authorization-server", 200, AUTH_METADATA)
        .once(Method::Post, "/register", 201, r#"{"client_id":"dyn1"}"#)
        .once(Method::Post, "/token", 200, r#"{"access_token":"at","refresh_token":"rt","expires_in":3600}"#)
        .once(Method::Post, "/token", 200, r#"{"access_token":"at2","expires_in":3600}"#);
    let Ok(chosen) = entered(json!({ "url": SERVER })) else { panic!("refused") };
    assert_eq!(chosen.0.without_sign_in(&http).await, Ok(None));
    let started = chosen.0.begin(&http, 4321).await;
    let Ok(started) = started else { panic!("{started:?}") };
    assert!(started.url.starts_with("https://auth.tools.test/authorize?"), "{}", started.url);
    assert!(started.url.contains("resource=https%3A%2F%2Ftools.test%2Fmcp"), "{}", started.url);
    let grant = chosen.finish(&http, &started, &started.state, "c1", now()).await;
    let Ok(grant) = grant else { panic!("{grant:?}") };
    assert_eq!(grant.extra["resource"], SERVER);
    let fresh = generic().0.refresh(&http, &grant, now()).await;
    assert_eq!(fresh.as_ref().map(|g| g.access_token.as_str()), Ok("at2"));
    assert_eq!(fresh.map(|g| g.extra["resource"].clone()), Ok(json!(SERVER)));
    let refreshed = http.sent_to(Method::Post, "/token")[1].body_text();
    assert!(refreshed.contains("grant_type=refresh_token") && refreshed.contains("resource=https"), "{refreshed}");
}

#[tokio::test]
async fn a_refresh_without_its_server_asks_to_sign_in_again() {
    let grant = Grant { refresh_token: Some("rt".into()), ..Grant::bearer("at") };
    let refused = generic().0.refresh(&FakeHttp::new(), &grant, now()).await;
    assert_eq!(refused.map_err(|error| error.class), Err(ErrorClass::Auth));
}

#[tokio::test]
async fn the_account_is_the_named_server_with_its_tool_count_and_its_tools_are_listed() {
    let provider = loaded("mcp");
    let http = listing(FakeHttp::new());
    let accounts = provider.accounts(&http, &named("at")).await;
    let Ok([account]) = accounts.as_deref() else { panic!("{accounts:?}") };
    assert_eq!(account.label, "Linear");
    assert_eq!(account.settings, json!({ "url": SERVER, "shown": { "tools": 2, "host": "tools.test" } }));
    let detail = crate::texts::detail(Language::Portuguese, &account.settings);
    assert_eq!(detail.as_deref(), Some("2 ferramentas · tools.test"));
    assert_eq!(http.sent()[0].header_value("authorization"), Some("Bearer at"));
    let http = listing(FakeHttp::new());
    let listed = provider.call(&http, &named(""), "mcp.tools", &json!({}), "k").await;
    assert_eq!(listed, Ok(json!([{ "name": "search" }, { "name": "create_issue" }])));
    let spec = provider.descriptor().action("mcp.tools").cloned();
    let Some(spec) = spec else { panic!("no action") };
    assert!(crate::descriptor::conforms(&spec.output_schema, &json!([])).is_ok());
}

#[tokio::test]
async fn an_unnamed_server_is_named_after_its_host() {
    let http = listing(FakeHttp::new());
    let unnamed = creds("", json!({ "url": SERVER }));
    let accounts = loaded("mcp").accounts(&http, &unnamed).await.unwrap_or_default();
    assert_eq!(accounts.first().map(|account| account.label.as_str()), Some("tools.test"));
}
