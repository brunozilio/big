pub mod sigv4;

use base64::Engine;
use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use ring::rand::{SecureRandom, SystemRandom};
use ring::{digest, hmac};
use serde_json::{Value, json};

pub const MOST_RANDOM: usize = 4096;

pub fn sha256(data: &[u8]) -> Vec<u8> {
    digest::digest(&digest::SHA256, data).as_ref().to_vec()
}

pub fn hmac_sha256(key: &[u8], data: &[u8]) -> Vec<u8> {
    hmac::sign(&hmac::Key::new(hmac::HMAC_SHA256, key), data).as_ref().to_vec()
}

pub fn random(count: usize) -> Result<Vec<u8>, String> {
    if count > MOST_RANDOM {
        return Err(format!("at most {MOST_RANDOM} random bytes at a time"));
    }
    let mut bytes = vec![0; count];
    SystemRandom::new().fill(&mut bytes).map_err(|_| "the system has no source of randomness".to_owned())?;
    Ok(bytes)
}

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn unhex(text: &str) -> Result<Vec<u8>, String> {
    let invalid = || "hex text must have an even number of 0-9 and a-f digits".to_owned();
    if !text.len().is_multiple_of(2) {
        return Err(invalid());
    }
    (0..text.len())
        .step_by(2)
        .map(|at| text.get(at..at + 2).and_then(|pair| u8::from_str_radix(pair, 16).ok()).ok_or_else(invalid))
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pkce {
    pub verifier: String,
    pub challenge: String,
}

pub fn pkce() -> Result<Pkce, String> {
    let verifier = URL_SAFE_NO_PAD.encode(random(32)?);
    let challenge = URL_SAFE_NO_PAD.encode(sha256(verifier.as_bytes()));
    Ok(Pkce { verifier, challenge })
}

pub fn bytes_of(value: &Value) -> Result<Vec<u8>, String> {
    match value {
        Value::String(text) => Ok(text.as_bytes().to_vec()),
        Value::Object(fields) => match (fields.get("hex"), fields.get("base64"), fields.get("base64url")) {
            (Some(Value::String(text)), _, _) => unhex(text),
            (_, Some(Value::String(text)), _) => STANDARD.decode(text).map_err(|error| error.to_string()),
            (_, _, Some(Value::String(text))) => {
                URL_SAFE_NO_PAD.decode(text.trim_end_matches('=')).map_err(|error| error.to_string())
            }
            _ => Err("bytes are a text or one of {hex}, {base64} or {base64url}".into()),
        },
        _ => Err("bytes are a text or one of {hex}, {base64} or {base64url}".into()),
    }
}

pub fn encoded(bytes: &[u8], encoding: Option<&str>) -> Result<String, String> {
    match encoding.unwrap_or("hex") {
        "hex" => Ok(hex(bytes)),
        "base64" => Ok(STANDARD.encode(bytes)),
        "base64url" => Ok(URL_SAFE_NO_PAD.encode(bytes)),
        "text" => String::from_utf8(bytes.to_vec()).map_err(|_| "the bytes are not UTF-8 text".to_owned()),
        other => Err(format!("unknown encoding {other}; use hex, base64, base64url or text")),
    }
}

fn field<'a>(arguments: &'a Value, name: &str) -> Result<&'a Value, String> {
    arguments.get(name).filter(|value| !value.is_null()).ok_or_else(|| format!("{name} is missing"))
}

fn encoding(arguments: &Value) -> Option<&str> {
    arguments.get("encoding").and_then(Value::as_str)
}

pub fn call(name: &str, arguments: &Value) -> Result<Value, String> {
    let answer = match name {
        "sha256" => encoded(&sha256(&bytes_of(field(arguments, "data")?)?), encoding(arguments))?,
        "hmac" => {
            let key = bytes_of(field(arguments, "key")?)?;
            encoded(&hmac_sha256(&key, &bytes_of(field(arguments, "data")?)?), encoding(arguments))?
        }
        "random" => {
            let count = arguments.get("bytes").and_then(Value::as_u64).unwrap_or(32);
            encoded(&random(usize::try_from(count).unwrap_or(usize::MAX))?, encoding(arguments))?
        }
        "encode" => encoded(&bytes_of(field(arguments, "data")?)?, encoding(arguments))?,
        "pkce" => {
            let pair = pkce()?;
            return Ok(json!({ "verifier": pair.verifier, "challenge": pair.challenge, "method": "S256" }));
        }
        "sigv4" => return sigv4::call(arguments),
        other => return Err(format!("crypto.{other} is not a function")),
    };
    Ok(Value::String(answer))
}

#[cfg(test)]
#[path = "crypto_tests.rs"]
mod tests;
