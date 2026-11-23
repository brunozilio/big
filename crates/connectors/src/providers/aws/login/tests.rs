use ring::signature::{ECDSA_P256_SHA256_FIXED, UnparsedPublicKey};

use super::*;
use crate::error::ErrorClass;
use crate::http::{FakeHttp, Method};

fn login() -> ConsoleLogin {
    ConsoleLogin { region: "sa-east-1".into() }
}

fn now() -> Timestamp {
    Timestamp::from_second(1_000).unwrap_or(Timestamp::UNIX_EPOCH)
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
    let started = login().begin(&FakeHttp::new(), 7420).await;
    let Ok(started) = started else { panic!("{started:?}") };
    let url = url::Url::parse(&started.url).unwrap_or_else(|_| panic!("bad url"));
    let query: Vec<(String, String)> = url.query_pairs().into_owned().collect();
    let get = |key: &str| query.iter().find(|(k, _)| k == key).map(|(_, v)| v.clone());
    assert!(started.url.starts_with("https://sa-east-1.signin.aws.amazon.com/v1/authorize?"));
    assert_eq!(get("client_id").as_deref(), Some(CLIENT_ID));
    assert_eq!(get("code_challenge_method").as_deref(), Some("SHA-256"));
    assert_eq!(get("scope").as_deref(), Some("openid"));
    assert_eq!(get("redirect_uri").as_deref(), Some("http://127.0.0.1:7420/oauth/callback"));
    assert_eq!(get("code_challenge"), Some(challenge(&started.verifier)));
}

#[tokio::test]
async fn the_code_is_exchanged_with_a_proof_of_possession_and_the_keys_are_kept() {
    let http = FakeHttp::new().once(Method::Post, "/v1/token", 200, &answer("rt1"));
    let flow = login();
    let Ok(started) = flow.begin(&http, 1).await else { panic!("begin failed") };
    let grant = flow.exchange(&http, &started, "code1", now()).await;
    let Ok(grant) = grant else { panic!("{grant:?}") };
    let keys = Keys::read(&grant.access_token).unwrap_or_else(|| panic!("no keys"));
    assert_eq!((keys.access_key_id.as_str(), keys.account_id.as_str()), ("ASIA1", "123456789012"));
    assert_eq!(grant.expires_at.map(|at| at.as_second()), Some(1_900));
    assert_eq!(grant.refresh_token.as_deref(), Some("rt1"));
    let sent = &http.sent()[0];
    assert_eq!(sent.url, "https://sa-east-1.signin.aws.amazon.com/v1/token");
    let body = sent.body_json();
    assert_eq!(body["grantType"], "authorization_code");
    assert_eq!(body["codeVerifier"], started.verifier.as_str());
    assert_eq!(body["redirectUri"], "http://127.0.0.1:1/oauth/callback");
    assert!(verified(sent.header_value("dpop").unwrap_or_default(), &sent.url));
}

#[tokio::test]
async fn a_refresh_reuses_the_proof_key_and_keeps_the_account() {
    let http = FakeHttp::new()
        .once(Method::Post, "/v1/token", 200, &answer("rt1"))
        .once(
            Method::Post,
            "/v1/token",
            200,
            &json!({ "tokenOutput": serde_json::from_str::<Value>(&answer("rt2")).unwrap_or_default() }).to_string(),
        )
        .once(Method::Post, "/v1/token", 400, r#"{"error":"TOKEN_EXPIRED","message":"expired"}"#);
    let flow = login();
    let Ok(started) = flow.begin(&http, 1).await else { panic!("begin failed") };
    let Ok(grant) = flow.exchange(&http, &started, "code1", now()).await else { panic!("exchange failed") };
    let fresh = flow.refresh(&http, &grant, now()).await;
    let Ok(fresh) = fresh else { panic!("{fresh:?}") };
    assert_eq!(fresh.refresh_token.as_deref(), Some("rt2"));
    assert_eq!(fresh.extra["proof_key"], grant.extra["proof_key"]);
    assert_eq!(Keys::read(&fresh.access_token).map(|k| k.account_id), Some("123456789012".to_owned()));
    let body = http.sent()[1].body_json();
    assert_eq!(body, json!({ "clientId": CLIENT_ID, "grantType": "refresh_token", "refreshToken": "rt1" }));
    let refused = flow.refresh(&http, &fresh, now()).await;
    assert_eq!(refused.map_err(|e| e.class), Err(ErrorClass::Auth));
}
