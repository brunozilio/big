use jiff::Timestamp;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

use super::{Grant, error_code, granted, number, refused, text, token_error};
use crate::error::ActionError;
use crate::http::{Http, HttpRequest, HttpResponse};

pub const DEVICE_GRANT: &str = "urn:ietf:params:oauth:grant-type:device_code";
pub const DEFAULT_INTERVAL: i64 = 5;
pub const DEFAULT_EXPIRES_IN: i64 = 900;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Dialect {
    #[default]
    Form,
    CamelJson,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct DeviceApp {
    pub client_id: String,
    #[serde(default)]
    pub client_secret: Option<String>,
    pub device_endpoint: String,
    pub token_endpoint: String,
    #[serde(default)]
    pub scopes: Vec<String>,
    #[serde(default)]
    pub start_fields: Vec<(String, String)>,
    #[serde(default)]
    pub dialect: Dialect,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DeviceCode {
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    pub verification_uri_complete: Option<String>,
    pub expires_in: i64,
    pub interval: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Polled {
    Pending,
    SlowDown { interval: Option<i64> },
    Expired,
    Denied,
    Authorized(Grant),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegisteredClient {
    pub client_id: String,
    pub client_secret: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SsoSpec {
    pub registration_endpoint: String,
    pub client_name: String,
    pub scopes: Vec<String>,
    pub start_field: String,
    pub region: String,
    pub device: DeviceApp,
}

impl SsoSpec {
    pub fn in_region(&self, region: &str) -> SsoSpec {
        let region = if region.trim().is_empty() { self.region.as_str() } else { region.trim() };
        let place = |url: &str| url.replace("{region}", region);
        let mut spec = self.clone();
        spec.registration_endpoint = place(&self.registration_endpoint);
        spec.device.device_endpoint = place(&self.device.device_endpoint);
        spec.device.token_endpoint = place(&self.device.token_endpoint);
        spec.region = region.to_owned();
        spec
    }

    pub fn device_for(&self, client: &RegisteredClient, start_url: &str) -> DeviceApp {
        let mut device = self.device.clone();
        device.client_id.clone_from(&client.client_id);
        device.client_secret = Some(client.client_secret.clone());
        if !start_url.is_empty() {
            device.start_fields.push((self.start_field.clone(), start_url.to_owned()));
        }
        device
    }
}

fn owned(fields: &[(&str, &str)]) -> Vec<(String, String)> {
    fields.iter().map(|(key, value)| ((*key).to_owned(), (*value).to_owned())).collect()
}

fn camel(key: &str) -> String {
    let mut out = String::new();
    let mut upper = false;
    for character in key.chars() {
        if character == '_' {
            upper = true;
        } else if upper {
            out.push(character.to_ascii_uppercase());
            upper = false;
        } else {
            out.push(character);
        }
    }
    out
}

fn polled(code: &str, answer: &Value) -> Option<Polled> {
    match code {
        "authorizationpending" | "authorizationpendingexception" => Some(Polled::Pending),
        "slowdown" | "slowdownexception" => Some(Polled::SlowDown { interval: number(answer, &["interval"]) }),
        "expiredtoken" | "expiredtokenexception" | "codeexpired" => Some(Polled::Expired),
        "accessdenied" | "accessdeniedexception" | "authorizationdeclined" => Some(Polled::Denied),
        _ => None,
    }
}

fn answered(response: HttpResponse) -> Result<Value, ActionError> {
    match token_error(&response) {
        Some(error) => Err(error),
        None => Ok(response.value()),
    }
}

impl DeviceApp {
    fn request(&self, url: &str, fields: &[(String, String)]) -> Result<HttpRequest, ActionError> {
        let request = HttpRequest::post(url).header("accept", "application/json");
        let mut all = owned(&[("client_id", &self.client_id)]);
        if let Some(secret) = &self.client_secret {
            all.extend(owned(&[("client_secret", secret)]));
        }
        all.extend(fields.iter().cloned());
        match self.dialect {
            Dialect::Form => {
                let pairs: Vec<(&str, &str)> = all.iter().map(|(key, value)| (key.as_str(), value.as_str())).collect();
                Ok(request.form(&pairs))
            }
            Dialect::CamelJson => {
                let body: Map<String, Value> =
                    all.into_iter().map(|(key, value)| (camel(&key), json!(value))).collect();
                request.json(&body)
            }
        }
    }

    fn scope_fields(&self) -> Vec<(String, String)> {
        if self.scopes.is_empty() { Vec::new() } else { owned(&[("scope", &self.scopes.join(" "))]) }
    }

    pub async fn start(&self, http: &dyn Http) -> Result<DeviceCode, ActionError> {
        let mut fields = self.scope_fields();
        fields.extend(self.start_fields.iter().cloned());
        let answer = answered(http.send(self.request(&self.device_endpoint, &fields)?).await?)?;
        let needed = |names: &[&str]| {
            text(&answer, names).ok_or_else(|| ActionError::decode(format!("the device answer has no {}", names[0])))
        };
        Ok(DeviceCode {
            device_code: needed(&["device_code", "deviceCode"])?,
            user_code: needed(&["user_code", "userCode"])?,
            verification_uri: needed(&["verification_uri", "verificationUri", "verification_url"])?,
            verification_uri_complete: text(&answer, &["verification_uri_complete", "verificationUriComplete"]),
            expires_in: number(&answer, &["expires_in", "expiresIn"]).unwrap_or(DEFAULT_EXPIRES_IN),
            interval: number(&answer, &["interval"]).unwrap_or(DEFAULT_INTERVAL).max(1),
        })
    }

    pub async fn poll(&self, http: &dyn Http, device_code: &str, now: Timestamp) -> Result<Polled, ActionError> {
        let fields = owned(&[("grant_type", DEVICE_GRANT), ("device_code", device_code)]);
        let response = http.send(self.request(&self.token_endpoint, &fields)?).await?;
        let answer = response.value();
        if let Some(code) = error_code(&answer) {
            return polled(&code, &answer).ok_or_else(|| refused(&code, &answer));
        }
        Ok(Polled::Authorized(granted(&answered(response)?, now)?))
    }

    pub async fn refresh(&self, http: &dyn Http, refresh_token: &str, now: Timestamp) -> Result<Grant, ActionError> {
        let mut fields = owned(&[("grant_type", "refresh_token"), ("refresh_token", refresh_token)]);
        if self.dialect == Dialect::Form {
            fields.extend(self.scope_fields());
        }
        let answer = answered(http.send(self.request(&self.token_endpoint, &fields)?).await?)?;
        let mut fresh = granted(&answer, now)?;
        if fresh.refresh_token.is_none() {
            fresh.refresh_token = Some(refresh_token.to_owned());
        }
        Ok(fresh)
    }
}

pub async fn register_client(http: &dyn Http, spec: &SsoSpec) -> Result<RegisteredClient, ActionError> {
    let registration = json!({
        "clientName": spec.client_name,
        "clientType": "public",
        "scopes": spec.scopes,
        "grantTypes": [DEVICE_GRANT, "refresh_token"],
    });
    let request = HttpRequest::post(&spec.registration_endpoint).json(&registration)?;
    let answer = answered(http.send(request).await?)?;
    let needed = |name: &str| {
        text(&answer, &[name]).ok_or_else(|| ActionError::decode(format!("the registration has no {name}")))
    };
    Ok(RegisteredClient { client_id: needed("clientId")?, client_secret: needed("clientSecret")? })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::ErrorClass;
    use crate::http::{FakeHttp, Method};

    fn form_app() -> DeviceApp {
        DeviceApp {
            client_id: "cid".into(),
            client_secret: None,
            device_endpoint: "https://login.example/device/code".into(),
            token_endpoint: "https://login.example/token".into(),
            scopes: vec!["repo".into()],
            start_fields: Vec::new(),
            dialect: Dialect::Form,
        }
    }

    fn sso() -> SsoSpec {
        SsoSpec {
            registration_endpoint: "https://oidc.example/client/register".into(),
            client_name: "app".into(),
            scopes: vec!["account:access".into()],
            start_field: "start_url".into(),
            region: "r1".into(),
            device: DeviceApp {
                client_id: String::new(),
                client_secret: None,
                device_endpoint: "https://oidc.example/device_authorization".into(),
                token_endpoint: "https://oidc.example/token".into(),
                scopes: Vec::new(),
                start_fields: Vec::new(),
                dialect: Dialect::CamelJson,
            },
        }
    }

    fn now() -> Timestamp {
        Timestamp::from_second(10).unwrap_or(Timestamp::UNIX_EPOCH)
    }

    #[tokio::test]
    async fn a_device_code_is_started_and_polled_until_authorized() {
        let http = FakeHttp::new()
            .once(
                Method::Post,
                "/device/code",
                200,
                r#"{"device_code":"dc","user_code":"ABCD","verification_uri":"https://v","interval":0}"#,
            )
            .once(Method::Post, "/token", 400, r#"{"error":"authorization_pending"}"#)
            .once(Method::Post, "/token", 400, r#"{"error":"slow_down","interval":10}"#)
            .once(Method::Post, "/token", 200, r#"{"access_token":"at","scope":"repo"}"#);
        let app = form_app();
        let code = app.start(&http).await;
        assert_eq!(code.as_ref().map(|c| (c.user_code.as_str(), c.interval)), Ok(("ABCD", 1)));
        assert_eq!(app.poll(&http, "dc", now()).await, Ok(Polled::Pending));
        assert_eq!(app.poll(&http, "dc", now()).await, Ok(Polled::SlowDown { interval: Some(10) }));
        let done = app.poll(&http, "dc", now()).await;
        assert!(matches!(done, Ok(Polled::Authorized(grant)) if grant.access_token == "at"));
        assert!(http.sent()[1].body_text().contains("device_code=dc"));
        assert!(http.sent()[0].body_text().contains("scope=repo"));
    }

    #[tokio::test]
    async fn an_unknown_poll_error_is_an_auth_refusal() {
        let http = FakeHttp::new().once(Method::Post, "/token", 400, r#"{"error":"bad_client"}"#);
        let polled = form_app().poll(&http, "dc", now()).await;
        assert_eq!(polled.map_err(|e| e.class), Err(ErrorClass::Auth));
    }

    #[tokio::test]
    async fn a_dynamic_client_is_registered_and_used_with_camel_json() {
        let http = FakeHttp::new()
            .once(Method::Post, "/client/register", 200, r#"{"clientId":"c1","clientSecret":"s1"}"#)
            .once(
                Method::Post,
                "/device_authorization",
                200,
                r#"{"deviceCode":"d","userCode":"U","verificationUri":"https://v","verificationUriComplete":"https://v?c=U","expiresIn":600,"interval":2}"#,
            )
            .once(Method::Post, "/token", 400, r#"{"error":"expired_token"}"#);
        let spec = sso();
        let client = register_client(&http, &spec).await;
        assert_eq!(client.as_ref().map(|c| c.client_id.as_str()), Ok("c1"));
        let Ok(client) = client else { panic!("no client") };
        let device = spec.device_for(&client, "https://portal.example/start");
        let code = device.start(&http).await;
        assert_eq!(code.map(|c| c.verification_uri_complete), Ok(Some("https://v?c=U".to_owned())));
        assert_eq!(device.poll(&http, "d", now()).await, Ok(Polled::Expired));
        let start = http.sent()[1].body_json();
        assert_eq!(start["clientId"], "c1");
        assert_eq!(start["clientSecret"], "s1");
        assert_eq!(start["startUrl"], "https://portal.example/start");
        assert_eq!(http.sent()[2].body_json()["grantType"], DEVICE_GRANT);
    }
}
