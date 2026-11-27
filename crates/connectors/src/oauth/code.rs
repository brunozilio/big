use async_trait::async_trait;
use jiff::Timestamp;
use serde_json::json;

use super::{Authorization, Grant, LoopbackSignIn, challenge, granted, random_token, token_error};
use crate::error::ActionError;
use crate::http::{Http, HttpRequest};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeSignIn {
    pub authorize_url: String,
    pub token_url: String,
    pub client_id: String,
    pub redirect: String,
    pub scopes: Vec<String>,
}

impl CodeSignIn {
    fn scope(&self) -> String {
        self.scopes.join(" ")
    }

    async fn token(&self, http: &dyn Http, fields: &[(&str, &str)], now: Timestamp) -> Result<Grant, ActionError> {
        let request = HttpRequest::post(&self.token_url).header("accept", "application/json").form(fields);
        let response = http.send(request).await?;
        if let Some(error) = token_error(&response) {
            return Err(error);
        }
        granted(&response.value(), now)
    }

    fn authorize(&self, redirect_uri: &str, state: &str, verifier: &str) -> Result<String, ActionError> {
        let mut url = url::Url::parse(&self.authorize_url)
            .map_err(|error| ActionError::permanent(format!("the authorize address is not valid: {error}")))?;
        url.query_pairs_mut()
            .append_pair("client_id", &self.client_id)
            .append_pair("response_type", "code")
            .append_pair("response_mode", "query")
            .append_pair("redirect_uri", redirect_uri)
            .append_pair("scope", &self.scope())
            .append_pair("state", state)
            .append_pair("code_challenge", &challenge(verifier))
            .append_pair("code_challenge_method", "S256");
        Ok(url.into())
    }
}

#[async_trait]
impl LoopbackSignIn for CodeSignIn {
    fn name(&self) -> &str {
        &self.client_id
    }

    fn callback_port(&self) -> Option<u16> {
        if super::templated(&self.redirect) {
            return None;
        }
        url::Url::parse(&self.redirect).ok()?.port()
    }

    async fn begin(&self, _http: &dyn Http, port: u16) -> Result<Authorization, ActionError> {
        let redirect_uri = super::redirect_for(&self.redirect, port);
        let state = random_token()?;
        let verifier = random_token()?;
        let url = self.authorize(&redirect_uri, &state, &verifier)?;
        let carried = json!({ "token_url": self.token_url, "client_id": self.client_id, "scope": self.scope() });
        Ok(Authorization { url, state, verifier, redirect_uri, carried })
    }

    async fn exchange(
        &self,
        http: &dyn Http,
        authorization: &Authorization,
        code: &str,
        now: Timestamp,
    ) -> Result<Grant, ActionError> {
        let scope = authorization.carried["scope"].as_str().unwrap_or_default();
        let fields = [
            ("grant_type", "authorization_code"),
            ("code", code),
            ("redirect_uri", authorization.redirect_uri.as_str()),
            ("code_verifier", authorization.verifier.as_str()),
            ("client_id", self.client_id.as_str()),
            ("scope", scope),
        ];
        let mut grant = self.token(http, &fields, now).await?;
        grant.extra = json!({ "scope": scope });
        Ok(grant)
    }

    async fn refresh(&self, http: &dyn Http, grant: &Grant, now: Timestamp) -> Result<Grant, ActionError> {
        let refresh = grant.refresh_token.as_deref().ok_or_else(|| ActionError::auth("there is no refresh token"))?;
        let scope = grant.extra["scope"].as_str().unwrap_or_default();
        let fields = [
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh),
            ("client_id", self.client_id.as_str()),
            ("scope", scope),
        ];
        let mut fresh = self.token(http, &fields, now).await?;
        fresh.refresh_token = fresh.refresh_token.or_else(|| grant.refresh_token.clone());
        fresh.extra = grant.extra.clone();
        fresh.method.clone_from(&grant.method);
        Ok(fresh)
    }
}

#[cfg(test)]
mod tests;
