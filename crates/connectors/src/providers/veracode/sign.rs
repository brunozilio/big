use platform::crypto::{hex, hmac_sha256};
use serde::Deserialize;

pub const SCHEME: &str = "VERACODE-HMAC-SHA-256";
const VERSION: &[u8] = b"vcode_request_version_1";

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Keys {
    pub api_id: String,
    pub api_key: String,
}

impl Keys {
    pub fn read(access_token: &str) -> Option<Keys> {
        let keys = serde_json::from_str::<Keys>(access_token).ok()?;
        let api_id = unprefixed(keys.api_id.trim()).to_owned();
        let api_key = unprefixed(keys.api_key.trim()).to_owned();
        (!api_id.is_empty() && !api_key.is_empty()).then_some(Keys { api_id, api_key })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Request<'a> {
    pub method: &'a str,
    pub host: &'a str,
    pub path_and_query: &'a str,
}

pub fn unprefixed(text: &str) -> &str {
    text.split_once('-').map_or(text, |(_, rest)| rest)
}

fn unhex(text: &str) -> Option<Vec<u8>> {
    if !text.len().is_multiple_of(2) {
        return None;
    }
    (0..text.len())
        .step_by(2)
        .map(|at| text.get(at..at + 2).and_then(|pair| u8::from_str_radix(pair, 16).ok()))
        .collect()
}

pub fn signature(keys: &Keys, request: Request<'_>, millis: i64, nonce: &[u8]) -> Option<String> {
    let data = format!(
        "id={}&host={}&url={}&method={}",
        keys.api_id.to_ascii_lowercase(),
        request.host.to_ascii_lowercase(),
        request.path_and_query,
        request.method.to_ascii_uppercase()
    );
    let key_nonce = hmac_sha256(&unhex(&keys.api_key)?, nonce);
    let key_date = hmac_sha256(&key_nonce, millis.to_string().as_bytes());
    let key_signature = hmac_sha256(&key_date, VERSION);
    Some(hex(&hmac_sha256(&key_signature, data.as_bytes())))
}

pub fn header(keys: &Keys, request: Request<'_>, millis: i64, nonce: &[u8]) -> Option<String> {
    let signed = signature(keys, request, millis, nonce)?;
    Some(format!("{SCHEME} id={},ts={millis},nonce={},sig={signed}", keys.api_id, hex(nonce)))
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
    const NONCE: [u8; 16] = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15];

    fn keys() -> Keys {
        Keys { api_id: "3ddaeeb10ca690df3fee5e3bd1c329fa".to_owned(), api_key: KEY.to_owned() }
    }

    fn listing() -> Request<'static> {
        Request { method: "GET", host: "api.veracode.com", path_and_query: "/appsec/v1/applications?page=0&size=100" }
    }

    #[test]
    fn the_signature_follows_the_veracode_key_chain() {
        assert_eq!(
            signature(&keys(), listing(), 1_700_000_000_123, &NONCE).as_deref(),
            Some("70efdd004ee88f27c06d7e38b108c0b1a71f2629a76a2976751bf85d142e4ad3")
        );
    }

    #[test]
    fn the_header_carries_the_id_the_time_the_nonce_and_the_signature() {
        let told = header(&keys(), listing(), 1_700_000_000_123, &NONCE).unwrap_or_default();
        assert_eq!(
            told,
            "VERACODE-HMAC-SHA-256 id=3ddaeeb10ca690df3fee5e3bd1c329fa,ts=1700000000123,nonce=000102030405060708090a0b0c0d0e0f,sig=70efdd004ee88f27c06d7e38b108c0b1a71f2629a76a2976751bf85d142e4ad3"
        );
    }

    #[test]
    fn a_key_that_is_not_hex_cannot_sign() {
        let bad = Keys { api_id: "a".to_owned(), api_key: "xyz".to_owned() };
        assert_eq!(signature(&bad, listing(), 1, &NONCE), None);
    }

    #[test]
    fn typed_keys_lose_their_prefix_and_spaces() {
        let read = Keys::read(r#"{"apiId": " vera01ki-abc ", "apiKey": "vera01ks-0f"}"#);
        assert_eq!(read, Some(Keys { api_id: "abc".to_owned(), api_key: "0f".to_owned() }));
        assert_eq!(Keys::read(r#"{"apiId": "abc", "apiKey": "0f"}"#).map(|k| k.api_id), Some("abc".to_owned()));
        assert_eq!(Keys::read(r#"{"apiId": "", "apiKey": "0f"}"#), None);
        assert_eq!(Keys::read("not json"), None);
    }
}
