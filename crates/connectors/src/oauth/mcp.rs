use async_trait::async_trait;
use domain::mcp_oauth::{self, AuthServer, Authorize};
use jiff::Timestamp;
use serde_json::{Value, json};

use super::{
    Authorization, Grant, LoopbackSignIn, RegisteredClient, challenge, granted, random_token, text, token_error,
};
use crate::error::ActionError;
use crate::http::{Http, HttpRequest};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpSignIn {
    pub server: String,
    pub redirect: String,
    pub client_name: String,
    pub scopes: Vec<String>,
}

async fn get_json(http: &dyn Http, url: &str) -> Option<Value> {
    let response = http.send(HttpRequest::get(url).header("accept", "application/json")).await.ok()?;
    (200..300).contains(&response.status).then(|| response.value()).filter(Value::is_object)
}

fn unsafe_address(url: &str) -> ActionError {
    ActionError::permanent(format!("{url} is not a safe address to sign in with"))
}

pub async fn discover(http: &dyn Http, server: &str) -> Result<(AuthServer, Vec<String>), ActionError> {
    if !mcp_oauth::endpoint_ok(server) {
        return Err(unsafe_address(server));
    }
    let mut resource = None;
    for url in mcp_oauth::resource_metadata_urls(server) {
        if let Some(found) = get_json(http, &url).await.filter(|found| found["authorization_servers"][0].is_string()) {
            resource = Some(found);
            break;
        }
    }
    let resource = resource.ok_or_else(|| ActionError::permanent(format!("{server} does not say how to sign in")))?;
    let issuer = resource["authorization_servers"][0].as_str().map(str::to_owned);
    let issuer = issuer.filter(|issuer| mcp_oauth::endpoint_ok(issuer)).ok_or_else(|| unsafe_address(server))?;
    let mut auth = None;
    for url in mcp_oauth::auth_server_metadata_urls(&issuer) {
        if let Some(found) = get_json(http, &url).await.and_then(|found| serde_json::from_value(found).ok()) {
            auth = Some(found);
            break;
        }
    }
    let auth: AuthServer = auth.ok_or_else(|| ActionError::permanent(format!("{issuer} has no OAuth metadata")))?;
    let endpoints =
        [Some(&auth.authorization_endpoint), Some(&auth.token_endpoint), auth.registration_endpoint.as_ref()];
    if let Some(bad) = endpoints.into_iter().flatten().find(|url| !mcp_oauth::endpoint_ok(url)) {
        return Err(unsafe_address(bad));
    }
    Ok((auth, mcp_oauth::resource_scopes(&resource)))
}

impl McpSignIn {
    fn scopes(&self, offered: &[String]) -> Vec<String> {
        if self.scopes.is_empty() {
            return offered.to_vec();
        }
        let wanted = self.scopes.iter().filter(|scope| offered.is_empty() || offered.contains(scope));
        wanted.cloned().collect()
    }

    async fn register(
        &self,
        http: &dyn Http,
        auth: &AuthServer,
        redirect: &str,
    ) -> Result<RegisteredClient, ActionError> {
        let endpoint = auth.registration_endpoint.as_deref().ok_or_else(|| {
            ActionError::permanent(format!("{} does not let applications register themselves", self.server))
        })?;
        let body = json!({
            "client_name": self.client_name,
            "redirect_uris": [redirect],
            "grant_types": ["authorization_code", "refresh_token"],
            "response_types": ["code"],
            "token_endpoint_auth_method": "none",
        });
        let response = http.send(HttpRequest::post(endpoint).header("accept", "application/json").json(&body)?).await?;
        if let Some(error) = token_error(&response) {
            return Err(error);
        }
        let answer = response.value();
        let client_id =
            text(&answer, &["client_id"]).ok_or_else(|| ActionError::decode("the registration has no client_id"))?;
        Ok(RegisteredClient { client_id, client_secret: text(&answer, &["client_secret"]).unwrap_or_default() })
    }

    async fn token(
        &self,
        http: &dyn Http,
        endpoint: &str,
        fields: &[(&str, &str)],
        now: Timestamp,
    ) -> Result<Grant, ActionError> {
        let request = HttpRequest::post(endpoint).header("accept", "application/json").form(fields);
        let response = http.send(request).await?;
        if let Some(error) = token_error(&response) {
            return Err(error);
        }
        granted(&response.value(), now)
    }
}

fn client_fields<'a>(client: &'a RegisteredClient, resource: &'a str) -> Vec<(&'a str, &'a str)> {
    let mut fields = vec![("client_id", client.client_id.as_str()), ("resource", resource)];
    if !client.client_secret.is_empty() {
        fields.push(("client_secret", client.client_secret.as_str()));
    }
    fields
}

#[async_trait]
impl LoopbackSignIn for McpSignIn {
    fn name(&self) -> &str {
        &self.server
    }

    async fn begin(&self, http: &dyn Http, port: u16) -> Result<Authorization, ActionError> {
        let (mut auth, offered) = discover(http, &self.server).await?;
        auth.resource_scopes = self.scopes(&offered);
        let redirect_uri = self.redirect.replace("{port}", &port.to_string());
        let client = self.register(http, &auth, &redirect_uri).await?;
        let state = random_token()?;
        let verifier = random_token()?;
        let url = Authorize {
            server: &auth,
            client_id: &client.client_id,
            redirect_uri: &redirect_uri,
            challenge: &challenge(&verifier),
            state: &state,
            resource: &self.server,
        }
        .url();
        let carried = json!({ "client": client, "token_endpoint": auth.token_endpoint });
        Ok(Authorization { url, state, verifier, redirect_uri, carried })
    }

    async fn exchange(
        &self,
        http: &dyn Http,
        authorization: &Authorization,
        code: &str,
        now: Timestamp,
    ) -> Result<Grant, ActionError> {
        let client: RegisteredClient = serde_json::from_value(authorization.carried["client"].clone())
            .map_err(|_| ActionError::auth("the sign-in lost its registered client"))?;
        let endpoint = authorization.carried["token_endpoint"].as_str().unwrap_or_default();
        let mut fields = vec![
            ("grant_type", "authorization_code"),
            ("code", code),
            ("redirect_uri", authorization.redirect_uri.as_str()),
            ("code_verifier", authorization.verifier.as_str()),
        ];
        fields.extend(client_fields(&client, &self.server));
        let mut grant = self.token(http, endpoint, &fields, now).await?;
        grant.client = Some(client);
        grant.extra = json!({ "token_endpoint": endpoint });
        Ok(grant)
    }

    async fn refresh(&self, http: &dyn Http, grant: &Grant, now: Timestamp) -> Result<Grant, ActionError> {
        let client = grant.client.as_ref().ok_or_else(|| ActionError::auth("the registered client is missing"))?;
        let refresh = grant.refresh_token.as_deref().ok_or_else(|| ActionError::auth("there is no refresh token"))?;
        let endpoint = grant.extra["token_endpoint"].as_str().unwrap_or_default();
        let mut fields = vec![("grant_type", "refresh_token"), ("refresh_token", refresh)];
        fields.extend(client_fields(client, &self.server));
        let mut fresh = self.token(http, endpoint, &fields, now).await?;
        fresh.refresh_token = fresh.refresh_token.or_else(|| grant.refresh_token.clone());
        fresh.client = Some(client.clone());
        fresh.extra = grant.extra.clone();
        fresh.method.clone_from(&grant.method);
        Ok(fresh)
    }
}

#[cfg(test)]
mod tests;
