use serde_json::{Value, json};

use crate::error::{ActionError, ErrorClass};
use crate::http::{FakeHttp, Method};
use crate::provider::{Credentials, Provider};
use crate::scripted::tests::{creds as made, loaded};

async fn checked(http: &FakeHttp, creds: &Credentials, action: &str, args: &Value) -> Result<Value, ActionError> {
    let provider = loaded("aws");
    let answer = provider.call(http, creds, action, args, "").await;
    if let Ok(value) = &answer {
        let spec = provider.descriptor().action(action).cloned().unwrap_or_else(|| panic!("no {action}"));
        assert_eq!(crate::descriptor::conforms(&spec.output_schema, value), Ok(()), "{action}: {value}");
    }
    answer
}

fn creds() -> Credentials {
    made("sso-at", json!({ "region": "eu-west-1" }))
}

fn console() -> Credentials {
    let keys = r#"{"accessKeyId":"ASIA1","secretAccessKey":"sk","sessionToken":"st","accountId":"123456789012","expiresAt":"2100-01-01T00:00:00Z"}"#;
    made(keys, json!({ "region": "us-east-1" }))
}

const CALLER: &str = "<GetCallerIdentityResponse><GetCallerIdentityResult><Arn>arn:aws:iam::123456789012:user/ana</Arn><UserId>AIDA</UserId><Account>123456789012</Account></GetCallerIdentityResult></GetCallerIdentityResponse>";

#[test]
fn the_cloud_actions_are_offered_without_the_last_deployment() {
    let names: Vec<String> = loaded("aws").descriptor().actions.into_iter().map(|action| action.name).collect();
    assert_eq!(names, ["cloud.identity", "cloud.accounts_and_roles", "cloud.options"]);
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
    let answer = checked(&http, &creds(), "cloud.accounts_and_roles", &json!({})).await;
    let Ok(answer) = answer else { panic!("{answer:?}") };
    assert_eq!(answer["accounts"][0]["roles"], json!(["Admin", "ReadOnly"]));
    assert_eq!(answer["accounts"][1]["name"], "dev");
    let sent = http.sent();
    assert!(sent[0].url.starts_with("https://portal.sso.eu-west-1.amazonaws.com/assignment/accounts"));
    assert!(sent[1].url.contains("next_token=n1"));
    assert_eq!(sent[0].header_value("x-amz-sso_bearer_token"), Some("sso-at"));
}

#[tokio::test]
async fn the_options_to_bind_are_each_account_and_role_the_identity_then_reads_back() {
    let http = FakeHttp::new()
        .once(Method::Get, "/assignment/accounts", 200, r#"{"accountList":[{"accountId":"111","accountName":"prod"}]}"#)
        .once(Method::Get, "account_id=111", 200, r#"{"roleList":[{"roleName":"Admin"},{"roleName":"ReadOnly"}]}"#)
        .once(
            Method::Get,
            "/federation/credentials?account_id=111&role_name=ReadOnly",
            200,
            r#"{"roleCredentials":{"accessKeyId":"ASIA","secretAccessKey":"s","sessionToken":"t"}}"#,
        );
    let options = checked(&http, &creds(), "cloud.options", &json!({})).await;
    let Ok(options) = options else { panic!("{options:?}") };
    assert_eq!(
        options,
        json!([{ "value": "111/Admin", "label": "prod · Admin" }, { "value": "111/ReadOnly", "label": "prod · ReadOnly" }])
    );
    let answer = checked(&http, &creds(), "cloud.identity", &json!({ "binding": "111/ReadOnly" })).await;
    let Ok(answer) = answer else { panic!("{answer:?}") };
    assert_eq!((&answer["account"], &answer["role"]), (&json!("111"), &json!("ReadOnly")));
}

#[tokio::test]
async fn identity_buys_role_credentials() {
    let http = FakeHttp::new().once(
        Method::Get,
        "/federation/credentials?account_id=111&role_name=ReadOnly",
        200,
        r#"{"roleCredentials":{"accessKeyId":"ASIA","secretAccessKey":"s","sessionToken":"t","expiration":4102444800000}}"#,
    );
    let answer = checked(&http, &creds(), "cloud.identity", &json!({"account": "111", "role": "ReadOnly"})).await;
    let Ok(answer) = answer else { panic!("{answer:?}") };
    assert_eq!(answer["access_key_id"], "ASIA");
    assert_eq!(answer["session_token"], "t");
    assert_eq!(answer["expires_at"], "2100-01-01T00:00:00Z");
}

#[tokio::test]
async fn a_refused_sso_token_is_an_auth_error() {
    let http = FakeHttp::new().once(Method::Get, "/federation/credentials", 401, "{}");
    let refused = checked(&http, &creds(), "cloud.identity", &json!({"account": "1", "role": "r"})).await;
    assert_eq!(refused.map_err(|e| e.class), Err(ErrorClass::Auth));
}

#[tokio::test]
async fn console_credentials_report_their_identity_through_sts() {
    let http = FakeHttp::new().always(Method::Post, "sts.us-east-1.amazonaws.com", 200, CALLER);
    let answer = checked(&http, &console(), "cloud.identity", &json!({})).await;
    let Ok(answer) = answer else { panic!("{answer:?}") };
    assert_eq!(answer["account"], "123456789012");
    assert_eq!(answer["arn"], "arn:aws:iam::123456789012:user/ana");
    assert_eq!(answer["access_key_id"], "ASIA1");
    assert_eq!(answer["expires_at"], "2100-01-01T00:00:00Z");
    assert!(http.sent()[0].header_value("authorization").is_some_and(|a| a.contains("Credential=ASIA1/")));
    assert_eq!(http.sent()[0].body_text(), "Action=GetCallerIdentity&Version=2011-06-15");
    let roles = checked(&http, &console(), "cloud.accounts_and_roles", &json!({})).await;
    assert_eq!(roles.map(|r| r["accounts"][0]["roles"].clone()), Ok(json!([])));
    let options = loaded("aws").accounts(&http, &console()).await;
    assert_eq!(options.map(|o| o[0].label.clone()), Ok("ana · 1234-5678-9012".to_owned()));
}

#[tokio::test]
async fn the_accounts_to_choose_are_the_portal_accounts() {
    let http = FakeHttp::new()
        .once(Method::Get, "/assignment/accounts", 200, r#"{"accountList":[{"accountId":"9","accountName":"lab"}]}"#)
        .once(Method::Get, "/assignment/roles", 200, r#"{"roleList":[{"roleName":"Admin"}]}"#);
    let options = loaded("aws").accounts(&http, &creds()).await;
    assert_eq!(
        options.map(|o| (o[0].label.clone(), o[0].settings["role"].clone())),
        Ok(("lab · 9".to_owned(), json!("Admin")))
    );
}

#[tokio::test]
async fn the_account_and_role_asked_for_win_over_the_ones_chosen_at_sign_in() {
    let chosen = made("sso-at", json!({ "region": "eu-west-1", "account": "111", "role": "Admin" }));
    let http = FakeHttp::new().once(
        Method::Get,
        "/federation/credentials?account_id=222&role_name=Deploy",
        200,
        r#"{"roleCredentials":{"accessKeyId":"ASIB","secretAccessKey":"s","sessionToken":"t","expiration":4102444800000}}"#,
    );
    let asked = checked(&http, &chosen, "cloud.identity", &json!({"account": "222", "role": "Deploy"})).await;
    assert_eq!(asked.map(|a| (a["account"].clone(), a["role"].clone())), Ok((json!("222"), json!("Deploy"))));
    let http = FakeHttp::new().once(
        Method::Get,
        "/federation/credentials?account_id=111&role_name=Admin",
        200,
        r#"{"roleCredentials":{"accessKeyId":"ASIC","secretAccessKey":"s","sessionToken":"t","expiration":4102444800000}}"#,
    );
    let kept = checked(&http, &chosen, "cloud.identity", &json!({})).await;
    assert_eq!(kept.map(|a| a["access_key_id"].clone()), Ok(json!("ASIC")));
}

#[tokio::test]
async fn console_credentials_answer_for_their_own_account_whatever_is_asked() {
    let http = FakeHttp::new().always(Method::Post, "sts.us-east-1.amazonaws.com", 200, CALLER);
    let answer = checked(&http, &console(), "cloud.identity", &json!({"account": "123456789012"})).await;
    assert_eq!(answer.map(|a| a["access_key_id"].clone()), Ok(json!("ASIA1")));
}

#[tokio::test]
async fn an_access_key_is_kept_as_json_keys() {
    let alternatives = loaded("aws").alternatives(domain::Language::Portuguese);
    let Some(crate::descriptor::AuthSpec::Token(spec)) = alternatives.get(1).map(|a| a.auth.clone()) else {
        panic!("not a token")
    };
    let token =
        spec.access_token(&json!({ "accessKeyId": "AKIA1", "secretAccessKey": "s" }), domain::Language::Portuguese);
    let token: Value = serde_json::from_str(&token.unwrap_or_default()).unwrap_or_default();
    assert_eq!(token, json!({ "accessKeyId": "AKIA1", "secretAccessKey": "s" }));
}
