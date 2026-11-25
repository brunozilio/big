use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use ring::signature::{ECDSA_P256_SHA256_FIXED, UnparsedPublicKey};
use serde_json::{Value, json};

use crate::descriptor::AuthSpec;
use crate::error::ErrorClass;
use crate::http::{FakeHttp, Method};
use crate::oauth::{Loopback, challenge};
use crate::provider::Provider;
use crate::scripted::tests::{loaded, now};

fn login() -> Loopback {
    let AuthSpec::Loopback(sign_in) = loaded("aws").descriptor().auth else { panic!("not loopback") };
    sign_in
}

fn keys_of(token: &str) -> Value {
    serde_json::from_str(token).unwrap_or_default()
}

fn id_token() -> String {
    let claims = json!({ "sub": "arn:aws:iam::123456789012:user/ana" });
    format!("e30.{}.sig", URL_SAFE_NO_PAD.encode(claims.to_string()))
}

fn answer(refresh: &str) -> String {
    json!({
        "accessToken": { "accessKeyId": "ASIA1", "secretAccessKey": "sk", "sessionToken": "st" },
        "tokenType": "aws_sigv4", "expiresIn": 900, "refreshToken": refresh, "idToken": id_token(),
    })
    .to_string()
}

fn verified(header: &str, uri: &str) -> bool {
    let parts: Vec<&str> = header.split('.').collect();
    let decode = |part: &str| -> Value {
        URL_SAFE_NO_PAD.decode(part).ok().and_then(|bytes| serde_json::from_slice(&bytes).ok()).unwrap_or_default()
    };
    let (head, body) = (decode(parts[0]), decode(parts[1]));
    let coordinate =
        |name: &str| URL_SAFE_NO_PAD.decode(head["jwk"][name].as_str().unwrap_or_default()).unwrap_or_default();
    let point = [vec![4], coordinate("x"), coordinate("y")].concat();
    let signature = URL_SAFE_NO_PAD.decode(parts[2]).unwrap_or_default();
    let input = format!("{}.{}", parts[0], parts[1]);
    head["typ"] == "dpop+jwt"
        && head["alg"] == "ES256"
        && body["htu"] == uri
        && body["htm"] == "POST"
        && UnparsedPublicKey::new(&ECDSA_P256_SHA256_FIXED, point).verify(input.as_bytes(), &signature).is_ok()
}

#[tokio::test]
async fn the_authorize_url_is_the_one_the_command_line_opens() {
    let started = login().0.begin(&FakeHttp::new(), 7420).await;
    let Ok(started) = started else { panic!("{started:?}") };
    let url = url::Url::parse(&started.url).unwrap_or_else(|_| panic!("bad url"));
    let query: Vec<(String, String)> = url.query_pairs().into_owned().collect();
    let get = |key: &str| query.iter().find(|(k, _)| k == key).map(|(_, v)| v.clone());
    assert!(started.url.starts_with("https://us-east-1.signin.aws.amazon.com/v1/authorize?"), "{}", started.url);
    assert_eq!(get("client_id").as_deref(), Some(brand::aws::LOGIN_CLIENT_ID));
    assert_eq!(get("code_challenge_method").as_deref(), Some("SHA-256"));
    assert_eq!(get("scope").as_deref(), Some("openid"));
    assert_eq!(get("redirect_uri").as_deref(), Some("http://127.0.0.1:7420/oauth/callback"));
    assert_eq!(get("code_challenge"), Some(challenge(&started.verifier)));
    assert_eq!(get("state").as_deref(), Some(started.state.as_str()));
    assert_eq!(login().0.name(), brand::aws::LOGIN_CLIENT_ID);
}

#[tokio::test]
async fn the_code_is_exchanged_with_a_proof_of_possession_and_the_keys_are_kept() {
    let http = FakeHttp::new().once(Method::Post, "/v1/token", 200, &answer("rt1"));
    let flow = login();
    let Ok(started) = flow.0.begin(&http, 1).await else { panic!("begin failed") };
    let grant = flow.finish(&http, &started, &started.state, "code1", now()).await;
    let Ok(grant) = grant else { panic!("{grant:?}") };
    let keys = keys_of(&grant.access_token);
    assert_eq!((keys["accessKeyId"].clone(), keys["accountId"].clone()), (json!("ASIA1"), json!("123456789012")));
    assert_eq!(keys["expiresAt"], "1970-01-01T00:31:40Z");
    assert_eq!(grant.expires_at.map(|at| at.as_second()), Some(1_900));
    assert_eq!(grant.refresh_token.as_deref(), Some("rt1"));
    assert_eq!(grant.extra["region"], "us-east-1");
    let sent = &http.sent()[0];
    assert_eq!(sent.url, "https://us-east-1.signin.aws.amazon.com/v1/token");
    let body = sent.body_json();
    assert_eq!(body["grantType"], "authorization_code");
    assert_eq!(body["code"], "code1");
    assert_eq!(body["codeVerifier"], started.verifier.as_str());
    assert_eq!(body["redirectUri"], "http://127.0.0.1:1/oauth/callback");
    assert!(verified(sent.header_value("dpop").unwrap_or_default(), &sent.url));
}

#[tokio::test]
async fn a_refresh_reuses_the_proof_key_and_keeps_the_account() {
    let wrapped = json!({ "tokenOutput": serde_json::from_str::<Value>(&answer("rt2")).unwrap_or_default() });
    let http = FakeHttp::new()
        .once(Method::Post, "/v1/token", 200, &answer("rt1"))
        .once(Method::Post, "/v1/token", 200, &wrapped.to_string())
        .once(Method::Post, "/v1/token", 400, r#"{"error":"TOKEN_EXPIRED","message":"expired"}"#);
    let flow = login();
    let Ok(started) = flow.0.begin(&http, 1).await else { panic!("begin failed") };
    let Ok(grant) = flow.0.exchange(&http, &started, "code1", now()).await else { panic!("exchange failed") };
    let fresh = flow.0.refresh(&http, &grant, now()).await;
    let Ok(fresh) = fresh else { panic!("{fresh:?}") };
    assert_eq!(fresh.refresh_token.as_deref(), Some("rt2"));
    assert_eq!(fresh.extra["proof_key"], grant.extra["proof_key"]);
    assert_eq!(keys_of(&fresh.access_token)["accountId"], "123456789012");
    let body = http.sent()[1].body_json();
    assert_eq!(
        body,
        json!({ "clientId": brand::aws::LOGIN_CLIENT_ID, "grantType": "refresh_token", "refreshToken": "rt1" })
    );
    assert!(verified(http.sent()[1].header_value("dpop").unwrap_or_default(), &http.sent()[1].url));
    let refused = flow.0.refresh(&http, &fresh, now()).await;
    assert_eq!(refused.as_ref().map_err(|e| e.class), Err(ErrorClass::Auth));
    assert!(refused.is_err_and(|error| error.message.contains("TOKEN_EXPIRED expired")));
}
