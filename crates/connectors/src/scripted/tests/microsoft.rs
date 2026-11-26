use serde_json::json;

use super::{creds, loaded, now};
use crate::descriptor::AuthSpec;
use crate::http::{FakeHttp, Method};
use crate::oauth::Loopback;
use crate::provider::Provider;

fn sign_in() -> Loopback {
    let AuthSpec::Loopback(sign_in) = loaded("microsoft").descriptor().auth else { panic!("not loopback") };
    sign_in
}

#[tokio::test]
async fn signing_in_opens_the_graph_authorize_endpoint_and_exchanges_the_code_for_a_grant() {
    let http = FakeHttp::new().once(
        Method::Post,
        "/oauth2/v2.0/token",
        200,
        r#"{"access_token":"at","refresh_token":"rt","expires_in":3600}"#,
    );
    let started = sign_in().0.begin(&http, 4321).await;
    let Ok(started) = started else { panic!("{started:?}") };
    assert!(
        started.url.starts_with("https://login.microsoftonline.com/common/oauth2/v2.0/authorize?"),
        "{}",
        started.url
    );
    let url = url::Url::parse(&started.url).unwrap_or_else(|_| panic!("bad url: {}", started.url));
    let query: Vec<(String, String)> = url.query_pairs().into_owned().collect();
    let get = |key: &str| query.iter().find(|(k, _)| k == key).map(|(_, v)| v.clone());
    assert_eq!(get("response_type").as_deref(), Some("code"));
    assert_eq!(get("code_challenge_method").as_deref(), Some("S256"));
    assert_eq!(get("redirect_uri").as_deref(), Some("http://127.0.0.1:4321/api/connections/oauth/callback"));
    assert!(get("scope").unwrap_or_default().contains("offline_access"), "{}", started.url);
    let grant = sign_in().finish(&http, &started, &started.state, "c1", now()).await;
    let Ok(grant) = grant else { panic!("{grant:?}") };
    assert_eq!(grant.refresh_token.as_deref(), Some("rt"));
    assert!(http.sent_to(Method::Post, "/oauth2/v2.0/token")[0].body_text().contains("grant_type=authorization_code"));
}

#[tokio::test]
async fn an_email_action_reaches_the_graph_messages_endpoint() {
    let provider = loaded("microsoft");
    let http = FakeHttp::new().always(Method::Get, "/me/messages", 200, r#"{"value":[{"id":"m1","subject":"Hi","from":{"emailAddress":{"address":"a@b.com"}},"receivedDateTime":"2026-01-01T00:00:00Z","isRead":false}]}"#);
    let answer =
        provider.call(&http, &creds("at", json!({})), "email.list_messages", &json!({ "limit": 5 }), "k").await;
    let Ok(listed) = answer else { panic!("{answer:?}") };
    assert_eq!(listed[0]["ref"], "m1");
    assert_eq!(listed[0]["from"], "a@b.com");
    assert!(http.sent()[0].url.contains("https://graph.microsoft.com/v1.0/me/messages"), "{}", http.sent()[0].url);
}
