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
fn the_sso_flow_registers_against_the_regional_oidc() {
    let AuthSpec::Sso(spec) = Aws.descriptor().auth else { panic!("not sso") };
    assert_eq!(
        spec.registration_endpoint,
        format!("https://oidc.{}.amazonaws.com/client/register", brand::connectors::AWS_SSO_REGION)
    );
    assert_eq!(spec.client_name, brand::NAME);
    assert_eq!(spec.scopes, vec![SCOPE.to_owned()]);
}

#[tokio::test]
async fn the_accounts_to_choose_are_the_portal_accounts() {
    let http = FakeHttp::new().once(
        Method::Get,
        "/assignment/accounts",
        200,
        r#"{"accountList":[{"accountId":"9","accountName":"lab"}]}"#,
    );
    let options = Aws.accounts(&http, &creds()).await;
    assert_eq!(
        options.map(|o| (o[0].label.clone(), o[0].settings["account"].clone())),
        Ok(("lab (9)".to_owned(), json!("9")))
    );
}
