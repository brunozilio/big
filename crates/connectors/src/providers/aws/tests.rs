use domain::connection::ConnectionId;
use serde_json::json;

use super::*;
use crate::error::ErrorClass;
use crate::http::{FakeHttp, Method};

fn creds() -> Credentials {
    Credentials::new(&ConnectionId::from("aws1"), "sso-at", json!({ "region": "eu-west-1" }))
}

#[tokio::test]
async fn accounts_and_roles_follow_every_page() {
    let http = FakeHttp::new()
        .once(
            Method::Get,
            "/assignment/accounts",
            200,
            r#"{"accountList":[{"accountId":"111","accountName":"prod","emailAddress":"p@x"}],"nextToken":"n1"}"#,
        )
        .once(Method::Get, "/assignment/accounts", 200, r#"{"accountList":[{"accountId":"222","accountName":"dev"}]}"#)
        .once(Method::Get, "account_id=111", 200, r#"{"roleList":[{"roleName":"Admin"},{"roleName":"ReadOnly"}]}"#)
        .once(Method::Get, "account_id=222", 200, r#"{"roleList":[]}"#);
    let answer = Aws.call(&http, &creds(), cloud::ACCOUNTS_AND_ROLES, &json!({}), "").await;
    let Ok(answer) = answer else { panic!("{answer:?}") };
    assert_eq!(answer["accounts"][0]["roles"], json!(["Admin", "ReadOnly"]));
    assert_eq!(answer["accounts"][1]["name"], "dev");
    let sent = http.sent();
    assert!(sent[0].url.starts_with("https://portal.sso.eu-west-1.amazonaws.com/assignment/accounts"));
    assert!(sent[1].url.contains("next_token=n1"));
    assert_eq!(sent[0].header_value("x-amz-sso_bearer_token"), Some("sso-at"));
}

#[tokio::test]
async fn identity_buys_role_credentials() {
    let http = FakeHttp::new().once(
        Method::Get,
        "/federation/credentials?account_id=111&role_name=ReadOnly",
        200,
        r#"{"roleCredentials":{"accessKeyId":"ASIA","secretAccessKey":"s","sessionToken":"t","expiration":4102444800000}}"#,
    );
    let answer = Aws.call(&http, &creds(), cloud::IDENTITY, &json!({"account": "111", "role": "ReadOnly"}), "").await;
    let Ok(answer) = answer else { panic!("{answer:?}") };
    assert_eq!(answer["access_key_id"], "ASIA");
    assert_eq!(answer["session_token"], "t");
    assert_eq!(answer["expires_at"], "2100-01-01T00:00:00Z");
}

#[tokio::test]
async fn a_refused_sso_token_is_an_auth_error() {
    let http = FakeHttp::new().once(Method::Get, "/federation/credentials", 401, "{}");
    let refused = Aws.call(&http, &creds(), cloud::IDENTITY, &json!({"account": "1", "role": "r"}), "").await;
    assert_eq!(refused.map_err(|e| e.class), Err(ErrorClass::Auth));
}

#[test]
fn the_console_sign_in_is_the_default_and_the_company_sso_the_alternative() {
    let AuthSpec::Loopback(_) = Aws.descriptor().auth else { panic!("not loopback") };
    let Some(alternative) = Aws.alternative() else { panic!("no alternative") };
    let AuthSpec::Sso(spec) = alternative.auth else { panic!("not sso") };
    assert_eq!(alternative.label, "Usar SSO da empresa");
    let spec = spec.in_region("eu-west-1");
    assert_eq!(spec.registration_endpoint, "https://oidc.eu-west-1.amazonaws.com/client/register");
    assert_eq!(spec.device.token_endpoint, "https://oidc.eu-west-1.amazonaws.com/token");
    assert_eq!(sso_spec().region, brand::connectors::AWS_REGION);
    assert_eq!(spec.client_name, brand::NAME);
    assert_eq!(spec.scopes, vec![SCOPE.to_owned()]);
}

fn console() -> Credentials {
    let keys = r#"{"accessKeyId":"ASIA1","secretAccessKey":"sk","sessionToken":"st","accountId":"123456789012","expiresAt":"2100-01-01T00:00:00Z"}"#;
    Credentials::new(&ConnectionId::from("aws2"), keys, json!({ "region": "us-east-1" }))
}

const CALLER: &str = "<GetCallerIdentityResponse><GetCallerIdentityResult><Arn>arn:aws:iam::123456789012:user/ana</Arn><UserId>AIDA</UserId><Account>123456789012</Account></GetCallerIdentityResult></GetCallerIdentityResponse>";

#[tokio::test]
async fn console_credentials_report_their_identity_through_sts() {
    let http = FakeHttp::new().always(Method::Post, "sts.us-east-1.amazonaws.com", 200, CALLER);
    let answer = Aws.call(&http, &console(), cloud::IDENTITY, &json!({}), "").await;
    let Ok(answer) = answer else { panic!("{answer:?}") };
    assert_eq!(answer["account"], "123456789012");
    assert_eq!(answer["arn"], "arn:aws:iam::123456789012:user/ana");
    assert_eq!(answer["access_key_id"], "ASIA1");
    assert_eq!(answer["expires_at"], "2100-01-01T00:00:00Z");
    assert!(http.sent()[0].header_value("authorization").is_some_and(|a| a.contains("Credential=ASIA1/")));
    let roles = Aws.call(&http, &console(), cloud::ACCOUNTS_AND_ROLES, &json!({}), "").await;
    assert_eq!(roles.map(|r| r["accounts"][0]["roles"].clone()), Ok(json!([])));
    let options = Aws.accounts(&http, &console()).await;
    assert_eq!(options.map(|o| o[0].label.clone()), Ok("ana · 1234-5678-9012".to_owned()));
}

#[tokio::test]
async fn the_accounts_to_choose_are_the_portal_accounts() {
    let http = FakeHttp::new()
        .once(Method::Get, "/assignment/accounts", 200, r#"{"accountList":[{"accountId":"9","accountName":"lab"}]}"#)
        .once(Method::Get, "/assignment/roles", 200, r#"{"roleList":[{"roleName":"Admin"}]}"#);
    let options = Aws.accounts(&http, &creds()).await;
    assert_eq!(
        options.map(|o| (o[0].label.clone(), o[0].settings["role"].clone())),
        Ok(("lab · 9".to_owned(), json!("Admin")))
    );
}
