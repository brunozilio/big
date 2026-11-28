use std::sync::Arc;

use serde_json::{Value, json};

use crate::error::{ActionError, ErrorClass};
use crate::http::{FakeHttp, Method};
use crate::scripted::bridge::{Answers, invoke};
use crate::scripted::host::{Local, Online};
use crate::scripted::sessions::McpSessions;
use crate::scripted::sign_in::redirect;
use crate::scripted::tests::loaded;

const ARN: &str = "arn:aws:secretsmanager:eu-west-1:123456789012:secret:rds!db-1";
const TASK: &str = "AWS_CONTAINER_CREDENTIALS_RELATIVE_URI";

async fn read(http: &FakeHttp, reference: &str, environment: Value) -> Result<Value, ActionError> {
    let provider = loaded("aws");
    let input = json!({ "reference": reference, "environment": environment });
    let online = Online::pooled(http, redirect(), Arc::new(McpSessions::default()), &input);
    let answers: Arc<dyn Answers> = Arc::new(Local);
    invoke(&provider.script, answers, &online, "secret", input).await
}

#[test]
fn the_connector_reads_secrets_and_names_the_task_variable() {
    let provider = loaded("aws");
    assert!(crate::provider::Provider::secrets(&provider).is_some());
    let names = provider.script.invoke_here("secretEnvironment", &json!({}), Arc::new(Local));
    assert_eq!(names.ok(), Some(json!([TASK])));
}

#[tokio::test]
async fn the_secret_is_read_with_the_task_credentials_and_signed_for_the_region() {
    let keys = r#"{"AccessKeyId":"ASIA1","SecretAccessKey":"sk","Token":"st","Expiration":"2100-01-01T00:00:00Z"}"#;
    let http = FakeHttp::new().once(Method::Get, "169.254.170.2/v2/credentials/abc", 200, keys).once(
        Method::Post,
        "secretsmanager.eu-west-1",
        200,
        r#"{"SecretString":"{\"password\":\"s3cr3t\"}"}"#,
    );
    let answer = read(&http, ARN, json!({ TASK: "/v2/credentials/abc" })).await;
    assert_eq!(answer, Ok(json!(r#"{"password":"s3cr3t"}"#)));
    let sent = http.sent();
    assert_eq!(sent[0].url, "http://169.254.170.2/v2/credentials/abc");
    assert_eq!(sent[1].url, "https://secretsmanager.eu-west-1.amazonaws.com/");
    assert_eq!(sent[1].header_value("x-amz-target"), Some("secretsmanager.GetSecretValue"));
    assert_eq!(sent[1].header_value("x-amz-security-token"), Some("st"));
    let authorization = sent[1].header_value("authorization").unwrap_or_default();
    assert!(authorization.contains("Credential=ASIA1/"), "{authorization}");
    assert!(authorization.contains("/eu-west-1/secretsmanager/aws4_request"), "{authorization}");
    assert_eq!(sent[1].body_json(), json!({ "SecretId": ARN }));
}

#[tokio::test]
async fn without_task_credentials_or_an_arn_the_secret_is_refused() {
    let outside = read(&FakeHttp::new(), ARN, json!({})).await;
    assert_eq!(outside.map_err(|error| error.class), Err(ErrorClass::Auth));
    let wrong = read(&FakeHttp::new(), "nope", json!({ TASK: "/x" })).await;
    assert!(wrong.is_err_and(|error| error.message.contains("nope: not a secret ARN")));
}
