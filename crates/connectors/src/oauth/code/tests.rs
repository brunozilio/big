use super::*;
use crate::http::{FakeHttp, Method};
use crate::oauth::Loopback;

fn sign_in() -> CodeSignIn {
    CodeSignIn {
        authorize_url: "https://login.test/common/authorize".into(),
        token_url: "https://login.test/common/token".into(),
        client_id: "cli".into(),
        redirect: "http://127.0.0.1:{port}/cb".into(),
        scopes: vec!["offline_access".into(), "Mail.Read".into()],
    }
}

fn now() -> Timestamp {
    Timestamp::from_second(1_000).unwrap_or(Timestamp::UNIX_EPOCH)
}

#[tokio::test]
async fn the_sign_in_builds_a_pkce_authorize_url_for_the_static_client() {
    let started = sign_in().begin(&FakeHttp::new(), 4321).await;
    let Ok(started) = started else { panic!("{started:?}") };
    let url = url::Url::parse(&started.url).unwrap_or_else(|_| panic!("bad url"));
    let query: Vec<(String, String)> = url.query_pairs().into_owned().collect();
    let get = |key: &str| query.iter().find(|(k, _)| k == key).map(|(_, v)| v.clone());
    assert!(started.url.starts_with("https://login.test/common/authorize?"));
    assert_eq!(get("client_id").as_deref(), Some("cli"));
    assert_eq!(get("response_type").as_deref(), Some("code"));
    assert_eq!(get("scope").as_deref(), Some("offline_access Mail.Read"));
    assert_eq!(get("redirect_uri").as_deref(), Some("http://127.0.0.1:4321/cb"));
    assert_eq!(get("code_challenge_method").as_deref(), Some("S256"));
    assert_eq!(get("code_challenge").as_deref(), Some(challenge(&started.verifier).as_str()));
    assert_eq!(get("state").as_deref(), Some(started.state.as_str()));
}

#[tokio::test]
async fn the_code_is_exchanged_and_later_refreshed_against_the_token_endpoint() {
    let http = FakeHttp::new()
        .once(Method::Post, "/common/token", 200, r#"{"access_token":"at","refresh_token":"rt","expires_in":3600}"#)
        .once(Method::Post, "/common/token", 200, r#"{"access_token":"at2","expires_in":3600}"#);
    let flow = sign_in();
    let Ok(started) = flow.begin(&http, 1).await else { panic!("begin failed") };
    let grant = Loopback::new(flow.clone()).finish(&http, &started, &started.state, "c1", now()).await;
    let Ok(grant) = grant else { panic!("{grant:?}") };
    let sent = http.sent_to(Method::Post, "/common/token")[0].body_text();
    assert!(sent.contains("grant_type=authorization_code"));
    assert!(sent.contains("code_verifier="));
    assert!(sent.contains("client_id=cli"));
    assert_eq!(grant.refresh_token.as_deref(), Some("rt"));
    let fresh = flow.refresh(&http, &grant, now()).await;
    assert_eq!(fresh.as_ref().map(|g| g.access_token.as_str()), Ok("at2"));
    assert_eq!(fresh.map(|g| g.refresh_token), Ok(Some("rt".to_owned())));
    let refreshed = http.sent_to(Method::Post, "/common/token")[1].body_text();
    assert!(refreshed.contains("grant_type=refresh_token") && refreshed.contains("refresh_token=rt"));
}

#[tokio::test]
async fn a_grant_without_a_refresh_token_cannot_be_refreshed() {
    let flow = sign_in();
    let refused = flow.refresh(&FakeHttp::new(), &Grant::bearer("at"), now()).await;
    assert!(refused.is_err());
}
