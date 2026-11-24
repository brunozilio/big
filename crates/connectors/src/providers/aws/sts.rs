use jiff::Timestamp;
use ring::hmac;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::error::ActionError;
use crate::http::{Http, HttpRequest, fetch};

const BODY: &str = "Action=GetCallerIdentity&Version=2011-06-15";
const FORM: &str = "application/x-www-form-urlencoded; charset=utf-8";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Keys {
    pub access_key_id: String,
    pub secret_access_key: String,
    #[serde(default)]
    pub session_token: String,
    #[serde(default)]
    pub account_id: String,
    #[serde(default)]
    pub expires_at: String,
}

impl Keys {
    pub fn read(access_token: &str) -> Option<Keys> {
        serde_json::from_str::<Keys>(access_token).ok().filter(|keys| !keys.access_key_id.is_empty())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Caller {
    pub account: String,
    pub arn: String,
    pub user_id: String,
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn digest(text: &str) -> String {
    hex(&Sha256::digest(text.as_bytes()))
}

fn mac(key: &[u8], data: &str) -> Vec<u8> {
    hmac::sign(&hmac::Key::new(hmac::HMAC_SHA256, key), data.as_bytes()).as_ref().to_vec()
}

pub fn signature(secret: &str, stamp: &str, region: &str, service: &str, canonical: &str) -> String {
    let day = &stamp[..stamp.len().min(8)];
    let scope = format!("{day}/{region}/{service}/aws4_request");
    let to_sign = format!("AWS4-HMAC-SHA256\n{stamp}\n{scope}\n{}", digest(canonical));
    let key = [day, region, service, "aws4_request"]
        .iter()
        .fold(format!("AWS4{secret}").into_bytes(), |key, part| mac(&key, part));
    hex(&mac(&key, &to_sign))
}

pub fn signed_identity_request(keys: &Keys, region: &str, now: Timestamp) -> HttpRequest {
    let host = format!("sts.{region}.amazonaws.com");
    let stamp = now.strftime("%Y%m%dT%H%M%SZ").to_string();
    let session = !keys.session_token.is_empty();
    let names =
        if session { "content-type;host;x-amz-date;x-amz-security-token" } else { "content-type;host;x-amz-date" };
    let token = if session { format!("x-amz-security-token:{}\n", keys.session_token) } else { String::new() };
    let canonical =
        format!("POST\n/\n\ncontent-type:{FORM}\nhost:{host}\nx-amz-date:{stamp}\n{token}\n{names}\n{}", digest(BODY));
    let signed = signature(&keys.secret_access_key, &stamp, region, "sts", &canonical);
    let day = &stamp[..8];
    let authorization = format!(
        "AWS4-HMAC-SHA256 Credential={}/{day}/{region}/sts/aws4_request, SignedHeaders={names}, Signature={signed}",
        keys.access_key_id
    );
    let request =
        HttpRequest::post(format!("https://{host}/")).body(FORM, BODY.as_bytes().to_vec()).header("x-amz-date", stamp);
    let request = if session { request.header("x-amz-security-token", keys.session_token.clone()) } else { request };
    request.header("authorization", authorization)
}

fn element(xml: &str, name: &str) -> String {
    let open = format!("<{name}>");
    let close = format!("</{name}>");
    xml.split_once(&open)
        .and_then(|(_, rest)| rest.split_once(&close))
        .map(|(inside, _)| inside.trim().to_owned())
        .unwrap_or_default()
}

pub async fn caller(http: &dyn Http, keys: &Keys, region: &str) -> Result<Caller, ActionError> {
    let answer = fetch(http, signed_identity_request(keys, region, Timestamp::now())).await?.text();
    let caller = Caller {
        account: element(&answer, "Account"),
        arn: element(&answer, "Arn"),
        user_id: element(&answer, "UserId"),
    };
    if caller.account.is_empty() {
        return Err(ActionError::decode("the identity has no account"));
    }
    Ok(caller)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_signature_matches_the_published_example() {
        let canonical = "GET\n/\nAction=ListUsers&Version=2010-05-08\ncontent-type:application/x-www-form-urlencoded; charset=utf-8\nhost:iam.amazonaws.com\nx-amz-date:20150830T123600Z\n\ncontent-type;host;x-amz-date\ne3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
        let signed =
            signature("wJalrXUtnFEMI/K7MDENG+bPxRfiCYEXAMPLEKEY", "20150830T123600Z", "us-east-1", "iam", canonical);
        assert_eq!(signed, "5d672d79c15b13162d9279b0855cfba6789a8edb4c82c400e06b5924a6f2b5d7");
    }

    #[test]
    fn the_identity_request_is_signed_for_sts_in_the_region() {
        let keys = Keys {
            access_key_id: "ASIA1".into(),
            secret_access_key: "s".into(),
            session_token: "t".into(),
            account_id: String::new(),
            expires_at: String::new(),
        };
        let now = Timestamp::from_second(1_700_000_000).unwrap_or(Timestamp::UNIX_EPOCH);
        let request = signed_identity_request(&keys, "sa-east-1", now);
        assert_eq!(request.url, "https://sts.sa-east-1.amazonaws.com/");
        let authorization = request.header_value("authorization").unwrap_or_default();
        assert!(authorization.starts_with("AWS4-HMAC-SHA256 Credential=ASIA1/20231114/sa-east-1/sts/aws4_request"));
        assert_eq!(request.header_value("x-amz-security-token"), Some("t"));
        assert_eq!(element("<a><Arn> arn:x </Arn></a>", "Arn"), "arn:x");
    }

    #[test]
    fn a_permanent_access_key_is_signed_without_a_session_token() {
        let Some(keys) = Keys::read(r#"{"accessKeyId":"AKIA1","secretAccessKey":"s"}"#) else { panic!("unread") };
        let now = Timestamp::from_second(1_700_000_000).unwrap_or(Timestamp::UNIX_EPOCH);
        let request = signed_identity_request(&keys, "us-east-1", now);
        assert_eq!(request.header_value("x-amz-security-token"), None);
        let authorization = request.header_value("authorization").unwrap_or_default();
        assert!(authorization.contains("SignedHeaders=content-type;host;x-amz-date,"), "{authorization}");
    }
}
