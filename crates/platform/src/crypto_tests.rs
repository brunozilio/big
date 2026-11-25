use serde_json::json;

use super::*;

#[test]
fn digests_match_the_published_vectors() {
    assert_eq!(hex(&sha256(b"")), "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");
    assert_eq!(
        hex(&hmac_sha256(b"Jefe", b"what do ya want for nothing?")),
        "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843"
    );
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
