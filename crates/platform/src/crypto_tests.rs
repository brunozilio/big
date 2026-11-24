use serde_json::json;

use super::sigv4::{Keys, Signing, sign, signature};
use super::*;

const SECRET: &str = "wJalrXUtnFEMI/K7MDENG+bPxRfiCYEXAMPLEKEY";

fn example_keys() -> Keys {
    Keys { access_key_id: "AKIDEXAMPLE".into(), secret_access_key: SECRET.into(), session_token: String::new() }
}

fn signed(method: &str, url: &str) -> String {
    let headers = [("Host".to_owned(), "example.amazonaws.com".to_owned())];
    let signing = Signing {
        method,
        url,
        headers: &headers,
        body: b"",
        region: "us-east-1",
        service: "service",
        stamp: "20150830T123600Z",
    };
    let added = sign(&signing, &example_keys()).unwrap_or_default();
    added.into_iter().find(|(name, _)| name == "authorization").map(|(_, value)| value).unwrap_or_default()
}

#[test]
fn digests_match_the_published_vectors() {
    assert_eq!(hex(&sha256(b"")), "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");
    assert_eq!(
        hex(&hmac_sha256(b"Jefe", b"what do ya want for nothing?")),
        "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843"
    );
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
    let signed = signature(SECRET, "20150830T123600Z", "us-east-1", "iam", canonical);
    assert_eq!(signed, "5d672d79c15b13162d9279b0855cfba6789a8edb4c82c400e06b5924a6f2b5d7");
}

#[test]
fn a_session_token_is_signed_and_returned() {
    let arguments = json!({
        "method": "POST",
        "url": "https://sts.sa-east-1.amazonaws.com/",
        "headers": { "content-type": "application/x-www-form-urlencoded; charset=utf-8" },
        "body": "Action=GetCallerIdentity&Version=2011-06-15",
        "region": "sa-east-1",
        "service": "sts",
        "stamp": "20231114T221320Z",
        "keys": { "accessKeyId": "ASIA1", "secretAccessKey": "s", "sessionToken": "t" },
    });
    let answer = call("sigv4", &arguments).unwrap_or_default();
    let headers = &answer["headers"];
    assert_eq!(headers["x-amz-security-token"], "t");
    assert_eq!(headers["x-amz-date"], "20231114T221320Z");
    let authorization = headers["authorization"].as_str().unwrap_or_default();
    assert!(authorization.starts_with("AWS4-HMAC-SHA256 Credential=ASIA1/20231114/sa-east-1/sts/aws4_request"));
    assert!(authorization.contains("SignedHeaders=content-type;host;x-amz-date;x-amz-security-token,"));
}

#[test]
fn the_script_calls_read_and_write_each_encoding() {
    assert_eq!(call("encode", &json!({ "data": "hi", "encoding": "base64" })), Ok(json!("aGk=")));
    assert_eq!(call("encode", &json!({ "data": { "base64": "aGk=" }, "encoding": "text" })), Ok(json!("hi")));
    assert_eq!(call("encode", &json!({ "data": { "hex": "6869" }, "encoding": "text" })), Ok(json!("hi")));
    assert_eq!(
        call("hmac", &json!({ "key": "Jefe", "data": "what do ya want for nothing?" })),
        Ok(json!("5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843"))
    );
    let random = call("random", &json!({ "bytes": 16 })).unwrap_or_default();
    assert_eq!(random.as_str().map(str::len), Some(32));
    assert!(call("random", &json!({ "bytes": 1 << 20 })).is_err());
    assert!(call("encode", &json!({ "data": { "hex": "zz" } })).is_err());
    assert!(call("nothing", &json!({})).is_err());
}

#[test]
fn pkce_pairs_a_verifier_with_its_challenge() {
    let pair = call("pkce", &json!({})).unwrap_or_default();
    let verifier = pair["verifier"].as_str().unwrap_or_default();
    let challenge = URL_SAFE_NO_PAD.encode(sha256(verifier.as_bytes()));
    assert_eq!(pair["challenge"], challenge.as_str());
    assert_eq!(pair["method"], "S256");
    assert_eq!(verifier.len(), 43);
}
