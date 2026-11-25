use async_trait::async_trait;
use base64::Engine;
use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use jiff::Timestamp;
use ring::rand::SystemRandom;
use ring::signature::{ECDSA_P256_SHA256_FIXED_SIGNING, EcdsaKeyPair, KeyPair};
use serde_json::{Value, json};

use super::sts::Keys;
use crate::clock::after_seconds;
use crate::error::ActionError;
use crate::http::{Http, HttpRequest};
use crate::oauth::{Authorization, Grant, LoopbackSignIn, challenge, random_token};

pub const CLIENT_ID: &str = brand::connectors::AWS_LOGIN_CLIENT_ID;
pub const REDIRECT: &str = "http://127.0.0.1:{port}/oauth/callback";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConsoleLogin {
    pub region: String,
}

pub fn base(region: &str) -> String {
    format!("https://{region}.signin.aws.amazon.com")
}

fn crypto(what: impl std::fmt::Display) -> ActionError {
    ActionError::permanent(format!("the proof key failed: {what}"))
}

pub fn new_key() -> Result<Vec<u8>, ActionError> {
    let pkcs8 = EcdsaKeyPair::generate_pkcs8(&ECDSA_P256_SHA256_FIXED_SIGNING, &SystemRandom::new()).map_err(crypto)?;
    Ok(pkcs8.as_ref().to_vec())
}

fn encoded(value: &Value) -> String {
    URL_SAFE_NO_PAD.encode(value.to_string().as_bytes())
}

pub fn proof(pkcs8: &[u8], uri: &str, now: Timestamp) -> Result<String, ActionError> {
    let random = SystemRandom::new();
    let pair = EcdsaKeyPair::from_pkcs8(&ECDSA_P256_SHA256_FIXED_SIGNING, pkcs8, &random).map_err(crypto)?;
    let point = pair.public_key().as_ref();
    if point.len() != 65 {
        return Err(crypto("unexpected public key"));
    }
    let jwk = json!({
        "kty": "EC", "x": URL_SAFE_NO_PAD.encode(&point[1..33]), "y": URL_SAFE_NO_PAD.encode(&point[33..65]), "crv": "P-256",
    });
    let header = json!({ "typ": "dpop+jwt", "alg": "ES256", "jwk": jwk });
    let payload = json!({ "htm": "POST", "htu": uri, "iat": now.as_second(), "jti": random_token()? });
    let input = format!("{}.{}", encoded(&header), encoded(&payload));
    let signed = pair.sign(&random, input.as_bytes()).map_err(crypto)?;
    Ok(format!("{input}.{}", URL_SAFE_NO_PAD.encode(signed.as_ref())))
}

fn account_of(id_token: &str) -> String {
    let payload = id_token.split('.').nth(1).and_then(|part| URL_SAFE_NO_PAD.decode(part.trim_end_matches('=')).ok());
    let claims: Value = payload.and_then(|bytes| serde_json::from_slice(&bytes).ok()).unwrap_or_default();
    claims["sub"].as_str().and_then(|sub| sub.split(':').nth(4)).unwrap_or_default().to_owned()
}

fn refused(status: u16, answer: &Value) -> ActionError {
    let code = answer["error"].as_str().unwrap_or_default();
    let message = answer["message"].as_str().or(answer["Message"].as_str()).unwrap_or_default();
    let told = format!("the sign-in answered {status}: {code} {message}").trim_end().to_owned();
    if status >= 500 {
        ActionError::transient(told)
    } else if status == 429 {
        ActionError::rate_limited(crate::error::DEFAULT_RETRY_AFTER, told)
    } else {
        ActionError::auth(told)
    }
}

impl ConsoleLogin {
    fn token_url(&self) -> String {
        format!("{}/v1/token", base(&self.region))
    }

    async fn token(&self, http: &dyn Http, key: &[u8], body: &Value, now: Timestamp) -> Result<Value, ActionError> {
        let url = self.token_url();
        let request =
            HttpRequest::post(&url).header("dpop", proof(key, &url, now)?).header("accept", "application/json");
        let response = http.send(request.json(body)?).await?;
        let answer = response.value();
        if !(200..300).contains(&response.status) {
            return Err(refused(response.status, &answer));
        }
        let output = answer.get("tokenOutput").cloned().unwrap_or(answer);
        if output["accessToken"]["accessKeyId"].as_str().is_none() {
            return Err(ActionError::decode("the sign-in gave no credentials"));
        }
        Ok(output)
    }

    fn grant(&self, output: &Value, key: &[u8], account: &str, now: Timestamp) -> Result<Grant, ActionError> {
        let expires_at = after_seconds(now, output["expiresIn"].as_i64().unwrap_or(900));
        let held = &output["accessToken"];
        let keys = Keys {
            access_key_id: held["accessKeyId"].as_str().unwrap_or_default().to_owned(),
            secret_access_key: held["secretAccessKey"].as_str().unwrap_or_default().to_owned(),
            session_token: held["sessionToken"].as_str().unwrap_or_default().to_owned(),
            account_id: account.to_owned(),
            expires_at: expires_at.to_string(),
        };
        let mut grant = Grant::bearer(&serde_json::to_string(&keys).map_err(ActionError::decode)?);
        grant.refresh_token = output["refreshToken"].as_str().map(str::to_owned);
        grant.expires_at = Some(expires_at);
        grant.extra = json!({ "proof_key": STANDARD.encode(key), "region": self.region, "client_id": CLIENT_ID });
        Ok(grant)
    }
}

#[async_trait]
impl LoopbackSignIn for ConsoleLogin {
    fn name(&self) -> &str {
        CLIENT_ID
    }

    async fn begin(&self, _http: &dyn Http, port: u16) -> Result<Authorization, ActionError> {
        let state = random_token()?;
        let verifier = random_token()?;
        let redirect_uri = REDIRECT.replace("{port}", &port.to_string());
        let mut url = url::Url::parse(&format!("{}/v1/authorize", base(&self.region)))
            .map_err(|error| ActionError::permanent(error.to_string()))?;
        url.query_pairs_mut()
            .append_pair("response_type", "code")
            .append_pair("client_id", CLIENT_ID)
            .append_pair("state", &state)
            .append_pair("code_challenge_method", "SHA-256")
            .append_pair("scope", "openid")
            .append_pair("redirect_uri", &redirect_uri)
            .append_pair("code_challenge", &challenge(&verifier));
        Ok(Authorization { url: url.into(), state, verifier, redirect_uri, carried: Value::Null })
    }

    async fn exchange(
        &self,
        http: &dyn Http,
        authorization: &Authorization,
        code: &str,
        now: Timestamp,
    ) -> Result<Grant, ActionError> {
        let key = new_key()?;
        let body = json!({
            "clientId": CLIENT_ID,
            "grantType": "authorization_code",
            "code": code,
            "codeVerifier": authorization.verifier,
            "redirectUri": authorization.redirect_uri,
        });
        let output = self.token(http, &key, &body, now).await?;
        let account = account_of(output["idToken"].as_str().unwrap_or_default());
        self.grant(&output, &key, &account, now)
    }

    async fn refresh(&self, http: &dyn Http, grant: &Grant, now: Timestamp) -> Result<Grant, ActionError> {
        let key = grant.extra["proof_key"].as_str().and_then(|text| STANDARD.decode(text).ok());
        let key = key.ok_or_else(|| ActionError::auth("the proof key is missing"))?;
        let refresh = grant.refresh_token.as_deref().ok_or_else(|| ActionError::auth("there is no refresh token"))?;
        let region = grant.extra["region"].as_str().unwrap_or(&self.region).to_owned();
        let signed_in = ConsoleLogin { region };
        let body = json!({ "clientId": CLIENT_ID, "grantType": "refresh_token", "refreshToken": refresh });
        let output = signed_in.token(http, &key, &body, now).await?;
        let account = Keys::read(&grant.access_token).map(|keys| keys.account_id).unwrap_or_default();
        let mut fresh = signed_in.grant(&output, &key, &account, now)?;
        fresh.method.clone_from(&grant.method);
        Ok(fresh)
    }
}

#[cfg(test)]
mod tests;
