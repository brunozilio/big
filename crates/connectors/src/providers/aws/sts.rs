use jiff::Timestamp;
use serde::{Deserialize, Serialize};
use platform::crypto::sigv4;

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

pub fn signed_identity_request(keys: &Keys, region: &str, now: Timestamp) -> HttpRequest {
    let url = format!("https://sts.{region}.amazonaws.com/");
    let headers = [("content-type".to_owned(), FORM.to_owned())];
    let stamp = sigv4::stamp_of(now);
    let signing = sigv4::Signing {
        method: "POST",
        url: &url,
        headers: &headers,
        body: BODY.as_bytes(),
        region,
        service: "sts",
        stamp: &stamp,
    };
    let signer = sigv4::Keys {
        access_key_id: keys.access_key_id.clone(),
        secret_access_key: keys.secret_access_key.clone(),
        session_token: keys.session_token.clone(),
    };
    let added = sigv4::sign(&signing, &signer).unwrap_or_default();
    added
        .into_iter()
        .fold(HttpRequest::post(url).body(FORM, BODY.as_bytes().to_vec()), |request, (name, value)| request.header(&name, value))
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
