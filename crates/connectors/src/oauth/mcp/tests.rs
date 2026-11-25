use super::*;
use crate::error::ErrorClass;
use crate::http::{FakeHttp, Method};
use crate::oauth::Loopback;

const RESOURCE: &str = r#"{"resource":"https://mcp.test/v2/mcp","authorization_servers":["https://auth.test/tenant"],"scopes_supported":["read:a","write:a","offline_access","admin"]}"#;
const SERVER: &str = r#"{"authorization_endpoint":"https://auth.test/authorize","token_endpoint":"https://auth.test/oauth/token","registration_endpoint":"https://auth.test/tenant/dcr/register"}"#;

fn sign_in() -> McpSignIn {
    McpSignIn {
        server: "https://mcp.test/v2/mcp".into(),
        redirect: "http://127.0.0.1:{port}/cb".into(),
        client_name: "app".into(),
        scopes: vec!["read:a".into(), "offline_access".into(), "missing".into()],
    }
}

fn discovered() -> FakeHttp {
    FakeHttp::new()
        .once(Method::Get, "/.well-known/oauth-protected-resource/v2/mcp", 200, RESOURCE)
        .once(Method::Get, "/.well-known/oauth-authorization-server/tenant", 200, SERVER)
        .once(Method::Post, "/dcr/register", 201, r#"{"client_id":"dyn1","client_secret":"sec1"}"#)
}

fn now() -> Timestamp {
    Timestamp::from_second(1_000).unwrap_or(Timestamp::UNIX_EPOCH)
}

#[tokio::test]
async fn the_sign_in_discovers_the_server_and_registers_itself() {
    let http = discovered();
    let started = sign_in().begin(&http, 4321).await;
    let Ok(started) = started else { panic!("{started:?}") };
    let url = url::Url::parse(&started.url).unwrap_or_else(|_| panic!("bad url"));
    let query: Vec<(String, String)> = url.query_pairs().into_owned().collect();
    let get = |key: &str| query.iter().find(|(k, _)| k == key).map(|(_, v)| v.clone());
    assert!(started.url.starts_with("https://auth.test/authorize?"));
    assert_eq!(get("client_id").as_deref(), Some("dyn1"));
    assert_eq!(get("scope").as_deref(), Some("read:a offline_access"));
    assert_eq!(get("resource").as_deref(), Some("https://mcp.test/v2/mcp"));
    assert_eq!(get("redirect_uri").as_deref(), Some("http://127.0.0.1:4321/cb"));
    let registration = http.sent_to(Method::Post, "/dcr/register")[0].body_json();
    assert_eq!(registration["redirect_uris"][0], "http://127.0.0.1:4321/cb");
    assert_eq!(registration["token_endpoint_auth_method"], "none");
}

#[tokio::test]
async fn the_code_and_later_refreshes_go_to_the_discovered_token_endpoint() {
    let http = discovered()
        .once(Method::Post, "/oauth/token", 200, r#"{"access_token":"at","refresh_token":"rt","expires_in":3600}"#)
        .once(Method::Post, "/oauth/token", 200, r#"{"access_token":"at2","expires_in":3600}"#);
    let flow = sign_in();
    let Ok(started) = flow.begin(&http, 1).await else { panic!("begin failed") };
    let grant = Loopback::new(flow.clone()).finish(&http, &started, &started.state, "c1", now()).await;
    let Ok(grant) = grant else { panic!("{grant:?}") };
    assert_eq!(grant.client.as_ref().map(|c| c.client_id.as_str()), Some("dyn1"));
    let sent = http.sent_to(Method::Post, "/oauth/token")[0].body_text();
    assert!(sent.contains("code_verifier=") && sent.contains("client_secret=sec1") && sent.contains("resource="));
    let fresh = flow.refresh(&http, &grant, now()).await;
    assert_eq!(fresh.as_ref().map(|g| g.access_token.as_str()), Ok("at2"));
    assert_eq!(fresh.map(|g| g.refresh_token), Ok(Some("rt".to_owned())));
    assert!(http.sent_to(Method::Post, "/oauth/token")[1].body_text().contains("grant_type=refresh_token"));
}

#[tokio::test]
async fn a_server_without_refresh_registers_for_codes_only_and_an_expired_grant_needs_a_new_sign_in() {
    let server = r#"{"authorization_endpoint":"https://auth.test/authorize","token_endpoint":"https://auth.test/oauth/token","registration_endpoint":"https://auth.test/tenant/dcr/register","grant_types_supported":["authorization_code"]}"#;
    let http = FakeHttp::new()
        .once(Method::Get, "/.well-known/oauth-protected-resource/v2/mcp", 200, RESOURCE)
        .once(Method::Get, "/.well-known/oauth-authorization-server/tenant", 200, server)
        .once(Method::Post, "/dcr/register", 201, r#"{"client_id":"dyn1"}"#)
        .once(Method::Post, "/oauth/token", 200, r#"{"access_token":"at","expires_in":3600}"#);
    let flow = sign_in();
    let Ok(started) = flow.begin(&http, 1).await else { panic!("begin failed") };
    let registration = http.sent_to(Method::Post, "/dcr/register")[0].body_json();
    assert_eq!(registration["grant_types"], json!(["authorization_code"]));
    let grant = Loopback::new(flow.clone()).finish(&http, &started, &started.state, "c1", now()).await;
    let Ok(grant) = grant else { panic!("{grant:?}") };
    assert_eq!(grant.refresh_token, None);
    let fresh = flow.refresh(&http, &grant, now()).await;
    assert_eq!(fresh.map_err(|error| error.class), Err(ErrorClass::Auth));
}

#[tokio::test]
async fn a_server_without_registration_cannot_be_signed_into() {
    let http = FakeHttp::new().once(Method::Get, "/.well-known/oauth-protected-resource/v2/mcp", 200, RESOURCE).once(
        Method::Get,
        "/.well-known/oauth-authorization-server/tenant",
        200,
        r#"{"authorization_endpoint":"https://auth.test/a","token_endpoint":"https://auth.test/t"}"#,
    );
    let refused = sign_in().begin(&http, 1).await;
    assert_eq!(refused.map_err(|e| e.class), Err(ErrorClass::Permanent));
}

#[tokio::test]
async fn an_insecure_server_is_refused_before_any_request() {
    let http = FakeHttp::new();
    let flow = McpSignIn { server: "http://evil.test/mcp".into(), ..sign_in() };
    assert!(flow.begin(&http, 1).await.is_err());
    assert!(http.sent().is_empty());
}

#[test]
fn a_server_that_only_accepts_a_fixed_redirect_gets_its_own_callback_port() {
    assert_eq!(sign_in().callback_port(), None);
    let fixed = McpSignIn { redirect: "http://127.0.0.1:33418/oauth/callback".into(), ..sign_in() };
    assert_eq!(fixed.callback_port(), Some(33418));
}
