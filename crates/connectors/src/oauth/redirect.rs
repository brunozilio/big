use jiff::Timestamp;
use serde_json::{Map, Value, json};

use super::{Grant, challenge, granted, random_token, token_error};
use crate::error::ActionError;
use crate::http::{Http, HttpRequest};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenBody {
    Form,
    Json,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OAuthApp {
    pub client_id: String,
    pub client_secret: Option<String>,
    pub redirect: String,
    pub authorize_endpoint: String,
    pub token_endpoint: String,
    pub scopes: Vec<String>,
    pub extra_authorize: Vec<(String, String)>,
    pub token_body: TokenBody,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Authorization {
    pub url: String,
    pub state: String,
    pub verifier: String,
    pub redirect_uri: String,
}

impl OAuthApp {
    pub fn redirect_uri(&self, port: u16) -> String {
        self.redirect.replace("{port}", &port.to_string())
    }

    pub fn configured(&self) -> bool {
        !self.client_id.is_empty()
    }

    pub fn begin(&self, port: u16) -> Result<Authorization, ActionError> {
        let state = random_token()?;
        let verifier = random_token()?;
        let redirect_uri = self.redirect_uri(port);
        let mut url =
            url::Url::parse(&self.authorize_endpoint).map_err(|error| ActionError::permanent(error.to_string()))?;
        {
            let mut query = url.query_pairs_mut();
            for (key, value) in &self.extra_authorize {
                query.append_pair(key, value);
            }
            query.append_pair("client_id", &self.client_id).append_pair("redirect_uri", &redirect_uri);
            if !self.scopes.is_empty() {
                query.append_pair("scope", &self.scopes.join(" "));
            }
            query
                .append_pair("state", &state)
                .append_pair("response_type", "code")
                .append_pair("code_challenge", &challenge(&verifier))
                .append_pair("code_challenge_method", "S256");
        }
        Ok(Authorization { url: url.into(), state, verifier, redirect_uri })
    }

    pub async fn exchange(
        &self,
        http: &dyn Http,
        authorization: &Authorization,
        state: &str,
        code: &str,
        now: Timestamp,
    ) -> Result<Grant, ActionError> {
        if state != authorization.state {
            return Err(ActionError::auth("the answer does not belong to this sign-in"));
        }
        let fields = [
            ("grant_type", "authorization_code"),
            ("code", code),
            ("redirect_uri", authorization.redirect_uri.as_str()),
            ("code_verifier", authorization.verifier.as_str()),
        ];
        self.token_request(http, &fields, now).await
    }

    pub async fn refresh(&self, http: &dyn Http, refresh_token: &str, now: Timestamp) -> Result<Grant, ActionError> {
        let fields = [("grant_type", "refresh_token"), ("refresh_token", refresh_token)];
        let mut grant = self.token_request(http, &fields, now).await?;
        if grant.refresh_token.is_none() {
            grant.refresh_token = Some(refresh_token.to_owned());
        }
        Ok(grant)
    }

    async fn token_request(
        &self,
        http: &dyn Http,
        fields: &[(&str, &str)],
        now: Timestamp,
    ) -> Result<Grant, ActionError> {
        let mut all = vec![("client_id", self.client_id.as_str())];
        if let Some(secret) = self.client_secret.as_deref().filter(|secret| !secret.is_empty()) {
            all.push(("client_secret", secret));
        }
        all.extend_from_slice(fields);
        let request = HttpRequest::post(&self.token_endpoint).header("accept", "application/json");
        let request = match self.token_body {
            TokenBody::Form => request.form(&all),
            TokenBody::Json => {
                let body: Map<String, Value> =
                    all.iter().map(|(key, value)| ((*key).to_owned(), json!(value))).collect();
                request.json(&body)?
            }
        };
        let response = http.send(request).await?;
        if let Some(error) = token_error(&response) {
            return Err(error);
        }
        granted(&response.value(), now)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::ErrorClass;
    use crate::http::{FakeHttp, Method};

    fn app(body: TokenBody) -> OAuthApp {
        OAuthApp {
            client_id: "cid".into(),
            client_secret: Some("sec".into()),
            redirect: "http://127.0.0.1:{port}/cb".into(),
            authorize_endpoint: "https://auth.example/authorize".into(),
            token_endpoint: "https://auth.example/token".into(),
            scopes: vec!["read".into(), "offline".into()],
            extra_authorize: vec![("audience".into(), "api".into())],
            token_body: body,
        }
    }

    fn now() -> Timestamp {
        Timestamp::from_second(1_000).unwrap_or(Timestamp::UNIX_EPOCH)
    }

    #[test]
    fn the_authorize_url_carries_pkce_state_and_the_loopback_port() {
        let started = app(TokenBody::Form).begin(4321);
        let Ok(started) = started else { panic!("begin failed") };
        assert_eq!(started.redirect_uri, "http://127.0.0.1:4321/cb");
        let url = url::Url::parse(&started.url).unwrap_or_else(|_| panic!("bad url"));
        let query: Vec<(String, String)> = url.query_pairs().into_owned().collect();
        let get = |key: &str| query.iter().find(|(k, _)| k == key).map(|(_, v)| v.clone());
        assert_eq!(get("audience").as_deref(), Some("api"));
        assert_eq!(get("scope").as_deref(), Some("read offline"));
        assert_eq!(get("state"), Some(started.state.clone()));
        assert_eq!(get("code_challenge"), Some(challenge(&started.verifier)));
        assert_eq!(get("code_challenge_method").as_deref(), Some("S256"));
        assert_eq!(get("redirect_uri").as_deref(), Some("http://127.0.0.1:4321/cb"));
    }

    #[tokio::test]
    async fn the_code_is_exchanged_with_the_verifier() {
        let http = FakeHttp::new().once(
            Method::Post,
            "/token",
            200,
            r#"{"access_token":"at","refresh_token":"rt","expires_in":3600}"#,
        );
        let app = app(TokenBody::Json);
        let Ok(started) = app.begin(1) else { panic!("begin failed") };
        let grant = app.exchange(&http, &started, &started.state, "code1", now()).await;
        assert_eq!(grant.as_ref().map(|g| g.access_token.as_str()), Ok("at"));
        assert_eq!(grant.map(|g| g.expires_at.map(|t| t.as_second())), Ok(Some(4_600)));
        let sent = http.sent()[0].body_json();
        assert_eq!(sent["code_verifier"], started.verifier.as_str());
        assert_eq!(sent["client_secret"], "sec");
        assert_eq!(sent["grant_type"], "authorization_code");
    }

    #[tokio::test]
    async fn a_foreign_state_is_refused_without_a_request() {
        let http = FakeHttp::new();
        let app = app(TokenBody::Form);
        let Ok(started) = app.begin(1) else { panic!("begin failed") };
        let refused = app.exchange(&http, &started, "other", "code", now()).await;
        assert_eq!(refused.map_err(|e| e.class), Err(ErrorClass::Auth));
        assert!(http.sent().is_empty());
    }

    #[tokio::test]
    async fn a_refresh_keeps_the_old_refresh_token_and_a_refusal_is_auth() {
        let http = FakeHttp::new().once(Method::Post, "/token", 200, r#"{"access_token":"new"}"#).once(
            Method::Post,
            "/token",
            400,
            r#"{"error":"invalid_grant"}"#,
        );
        let app = app(TokenBody::Form);
        let fresh = app.refresh(&http, "rt", now()).await;
        assert_eq!(fresh.map(|g| g.refresh_token), Ok(Some("rt".to_owned())));
        assert!(http.sent()[0].body_text().contains("grant_type=refresh_token"));
        let refused = app.refresh(&http, "rt", now()).await;
        assert_eq!(refused.map_err(|e| e.class), Err(ErrorClass::Auth));
    }
}
