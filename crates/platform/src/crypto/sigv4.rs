use serde::Deserialize;
use serde_json::{Value, json};

use super::{bytes_of, hex, hmac_sha256, sha256};

const ALGORITHM: &str = "AWS4-HMAC-SHA256";
const STAMP: &str = "%Y%m%dT%H%M%SZ";

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Keys {
    pub access_key_id: String,
    pub secret_access_key: String,
    #[serde(default)]
    pub session_token: String,
}

#[derive(Debug, Clone, Copy)]
pub struct Signing<'a> {
    pub method: &'a str,
    pub url: &'a str,
    pub headers: &'a [(String, String)],
    pub body: &'a [u8],
    pub region: &'a str,
    pub service: &'a str,
    pub stamp: &'a str,
}

pub fn signature(secret: &str, stamp: &str, region: &str, service: &str, canonical: &str) -> String {
    let day = &stamp[..stamp.len().min(8)];
    let scope = format!("{day}/{region}/{service}/aws4_request");
    let to_sign = format!("{ALGORITHM}\n{stamp}\n{scope}\n{}", hex(&sha256(canonical.as_bytes())));
    let key = [day, region, service, "aws4_request"]
        .iter()
        .fold(format!("AWS4{secret}").into_bytes(), |key, part| hmac_sha256(&key, part.as_bytes()));
    hex(&hmac_sha256(&key, to_sign.as_bytes()))
}

fn encode(text: &str) -> String {
    text.bytes()
        .map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => char::from(byte).to_string(),
            other => format!("%{other:02X}"),
        })
        .collect()
}

fn decoded(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut at = 0;
    while at < bytes.len() {
        let escaped = (bytes[at] == b'%')
            .then(|| text.get(at + 1..at + 3))
            .flatten()
            .and_then(|pair| u8::from_str_radix(pair, 16).ok());
        match escaped {
            Some(byte) => {
                out.push(byte);
                at += 3;
            }
            None => {
                out.push(bytes[at]);
                at += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn query_part(text: &str) -> String {
    decoded(&text.replace('+', " "))
}

fn path(url: &url::Url, service: &str) -> String {
    let path = if url.path().is_empty() { "/" } else { url.path() };
    if service == "s3" {
        return path.to_owned();
    }
    path.split('/')
        .map(|segment| encode(&decoded(segment)))
        .map(|segment| encode(&segment))
        .collect::<Vec<_>>()
        .join("/")
}

fn query(url: &url::Url) -> String {
    let mut pairs: Vec<(String, String)> = url
        .query()
        .unwrap_or_default()
        .split('&')
        .filter(|pair| !pair.is_empty())
        .map(|pair| pair.split_once('=').unwrap_or((pair, "")))
        .map(|(key, value)| (encode(&query_part(key)), encode(&query_part(value))))
        .collect();
    pairs.sort();
    pairs.iter().map(|(key, value)| format!("{key}={value}")).collect::<Vec<_>>().join("&")
}

fn host(url: &url::Url) -> String {
    let name = url.host_str().unwrap_or_default();
    match url.port() {
        Some(port) => format!("{name}:{port}"),
        None => name.to_owned(),
    }
}

fn canonical_headers(signing: &Signing<'_>, url: &url::Url, added: &[(String, String)]) -> Vec<(String, String)> {
    let mut headers: Vec<(String, String)> = signing
        .headers
        .iter()
        .chain(added)
        .map(|(name, value)| (name.trim().to_ascii_lowercase(), value.split_whitespace().collect::<Vec<_>>().join(" ")))
        .collect();
    if !headers.iter().any(|(name, _)| name == "host") {
        headers.push(("host".to_owned(), host(url)));
    }
    headers.sort();
    headers.dedup_by(|later, earlier| {
        let same = later.0 == earlier.0;
        if same {
            earlier.1 = format!("{},{}", earlier.1, later.1);
        }
        same
    });
    headers
}

pub fn sign(signing: &Signing<'_>, keys: &Keys) -> Result<Vec<(String, String)>, String> {
    let url = url::Url::parse(signing.url).map_err(|error| format!("{}: {error}", signing.url))?;
    let mut added = vec![("x-amz-date".to_owned(), signing.stamp.to_owned())];
    if !keys.session_token.is_empty() {
        added.push(("x-amz-security-token".to_owned(), keys.session_token.clone()));
    }
    let headers = canonical_headers(signing, &url, &added);
    let payload = headers
        .iter()
        .find(|(name, _)| name == "x-amz-content-sha256")
        .map_or_else(|| hex(&sha256(signing.body)), |(_, value)| value.clone());
    let names = headers.iter().map(|(name, _)| name.as_str()).collect::<Vec<_>>().join(";");
    let listed: String = headers.iter().map(|(name, value)| format!("{name}:{value}\n")).collect();
    let canonical = format!(
        "{}\n{}\n{}\n{listed}\n{names}\n{payload}",
        signing.method.to_ascii_uppercase(),
        path(&url, signing.service),
        query(&url)
    );
    let signed = signature(&keys.secret_access_key, signing.stamp, signing.region, signing.service, &canonical);
    let day = &signing.stamp[..signing.stamp.len().min(8)];
    let authorization = format!(
        "{ALGORITHM} Credential={}/{day}/{}/{}/aws4_request, SignedHeaders={names}, Signature={signed}",
        keys.access_key_id, signing.region, signing.service
    );
    added.push(("authorization".to_owned(), authorization));
    Ok(added)
}

pub fn stamp_of(now: jiff::Timestamp) -> String {
    now.strftime(STAMP).to_string()
}

fn text<'a>(arguments: &'a Value, name: &str) -> Result<&'a str, String> {
    arguments.get(name).and_then(Value::as_str).ok_or_else(|| format!("{name} must be a string"))
}

fn header_pairs(value: Option<&Value>) -> Vec<(String, String)> {
    match value {
        Some(Value::Object(fields)) => {
            fields.iter().filter_map(|(name, value)| Some((name.clone(), value.as_str()?.to_owned()))).collect()
        }
        Some(Value::Array(pairs)) => pairs
            .iter()
            .filter_map(|pair| Some((pair.get(0)?.as_str()?.to_owned(), pair.get(1)?.as_str()?.to_owned())))
            .collect(),
        _ => Vec::new(),
    }
}

pub fn call(arguments: &Value) -> Result<Value, String> {
    let keys: Keys = serde_json::from_value(arguments.get("keys").cloned().unwrap_or_default())
        .map_err(|error| format!("keys must have accessKeyId and secretAccessKey: {error}"))?;
    let headers = header_pairs(arguments.get("headers"));
    let body = match arguments.get("body") {
        None | Some(Value::Null) => Vec::new(),
        Some(body) => bytes_of(body)?,
    };
    let stamp = match arguments.get("stamp").and_then(Value::as_str) {
        Some(stamp) => stamp.to_owned(),
        None => stamp_of(jiff::Timestamp::now()),
    };
    let signing = Signing {
        method: arguments.get("method").and_then(Value::as_str).unwrap_or("GET"),
        url: text(arguments, "url")?,
        headers: &headers,
        body: &body,
        region: text(arguments, "region")?,
        service: text(arguments, "service")?,
        stamp: &stamp,
    };
    let added = sign(&signing, &keys)?;
    let headers: serde_json::Map<String, Value> =
        added.into_iter().map(|(name, value)| (name, Value::String(value))).collect();
    Ok(json!({ "headers": headers }))
}
