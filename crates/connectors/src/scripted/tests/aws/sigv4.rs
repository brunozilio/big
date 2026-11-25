use serde_json::{Value, json};

use super::here;

const SECRET: &str = "wJalrXUtnFEMI/K7MDENG+bPxRfiCYEXAMPLEKEY";

fn header(added: &Value, name: &str) -> String {
    let pairs = added.as_array().cloned().unwrap_or_default();
    let found = pairs.iter().find(|pair| pair[0] == name);
    found.and_then(|pair| pair[1].as_str()).unwrap_or_default().to_owned()
}

fn signed(method: &str, url: &str) -> String {
    let signing = json!({
        "method": method, "url": url, "headers": [["Host", "example.amazonaws.com"]], "body": "",
        "region": "us-east-1", "service": "service", "stamp": "20150830T123600Z",
    });
    let keys = json!({ "accessKeyId": "AKIDEXAMPLE", "secretAccessKey": SECRET });
    header(&here("signed", &json!({ "signing": signing, "keys": keys })), "authorization")
}

#[test]
fn sigv4_matches_the_aws_test_suite() {
    let expect = |signature: &str| {
        format!(
            "AWS4-HMAC-SHA256 Credential=AKIDEXAMPLE/20150830/us-east-1/service/aws4_request, \
             SignedHeaders=host;x-amz-date, Signature={signature}"
        )
    };
    assert_eq!(
        signed("GET", "https://example.amazonaws.com/"),
        expect("5fa00fa31553b73ebf1942676e86291e8372ff2a2260956d9b8aae1d763fbf31")
    );
    assert_eq!(
        signed("POST", "https://example.amazonaws.com/"),
        expect("5da7c1a2acd57cee7505fc6676e4e544621c30862966e37dddb68e92efbe5d6b")
    );
    assert_eq!(
        signed("GET", "https://example.amazonaws.com/?Param2=value2&Param1=value1"),
        expect("b97d918cfa904a5beff61c982a1b6f458b799221646efd99d3219ec94cdf2500")
    );
}

#[test]
fn the_signature_matches_the_iam_example() {
    let canonical = "GET\n/\nAction=ListUsers&Version=2010-05-08\ncontent-type:application/x-www-form-urlencoded; charset=utf-8\nhost:iam.amazonaws.com\nx-amz-date:20150830T123600Z\n\ncontent-type;host;x-amz-date\ne3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
    let input = json!({
        "secret": SECRET, "stamp": "20150830T123600Z", "region": "us-east-1", "service": "iam", "canonical": canonical,
    });
    assert_eq!(here("signatureOf", &input), "5d672d79c15b13162d9279b0855cfba6789a8edb4c82c400e06b5924a6f2b5d7");
}

#[test]
fn a_session_token_is_signed_and_returned() {
    let signing = json!({
        "method": "POST",
        "url": "https://sts.sa-east-1.amazonaws.com/",
        "headers": [["content-type", "application/x-www-form-urlencoded; charset=utf-8"]],
        "body": "Action=GetCallerIdentity&Version=2011-06-15",
        "region": "sa-east-1",
        "service": "sts",
        "stamp": "20231114T221320Z",
    });
    let keys = json!({ "accessKeyId": "ASIA1", "secretAccessKey": "s", "sessionToken": "t" });
    let added = here("signed", &json!({ "signing": signing, "keys": keys }));
    assert_eq!(header(&added, "x-amz-security-token"), "t");
    assert_eq!(header(&added, "x-amz-date"), "20231114T221320Z");
    let authorization = header(&added, "authorization");
    assert!(authorization.starts_with("AWS4-HMAC-SHA256 Credential=ASIA1/20231114/sa-east-1/sts/aws4_request"));
    assert!(authorization.contains("SignedHeaders=content-type;host;x-amz-date;x-amz-security-token,"));
}

#[test]
fn the_identity_request_is_signed_for_sts_in_the_region() {
    let token = r#"{"accessKeyId":"ASIA1","secretAccessKey":"s","sessionToken":"t"}"#;
    let request = here("identityRequest", &json!({ "token": token, "region": "sa-east-1", "second": 1_700_000_000 }));
    assert_eq!(request["url"], "https://sts.sa-east-1.amazonaws.com/");
    let authorization = request["headers"]["authorization"].as_str().unwrap_or_default();
    assert!(authorization.starts_with("AWS4-HMAC-SHA256 Credential=ASIA1/20231114/sa-east-1/sts/aws4_request"));
    assert_eq!(request["headers"]["x-amz-security-token"], "t");
    assert_eq!(here("elementOf", &json!({ "xml": "<a><Arn> arn:x </Arn></a>", "name": "Arn" })), "arn:x");
}

#[test]
fn a_permanent_access_key_is_signed_without_a_session_token() {
    let token = r#"{"accessKeyId":"AKIA1","secretAccessKey":"s"}"#;
    let request = here("identityRequest", &json!({ "token": token, "region": "us-east-1", "second": 1_700_000_000 }));
    assert!(request["headers"]["x-amz-security-token"].is_null());
    let authorization = request["headers"]["authorization"].as_str().unwrap_or_default();
    assert!(authorization.contains("SignedHeaders=content-type;host;x-amz-date,"), "{authorization}");
}
