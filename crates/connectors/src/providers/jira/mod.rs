mod adf;
mod events;
mod tracker;

use async_trait::async_trait;
use domain::connection::{Capability, ProviderKey};
use serde_json::{Value, json};

use crate::contracts::{self, tracker as actions};
use crate::descriptor::{AuthSpec, ProviderDescriptor};
use crate::error::ActionError;
use crate::http::{Http, HttpRequest, Method, fetch, fetch_json};
use crate::oauth::{OAuthApp, TokenBody};
use crate::provider::{AccountOption, Args, Credentials, EventSource, Provider, text_of, unknown_action};

pub const KEY: &str = "jira";
pub const DEFAULT_FLAG_FIELD: &str = "customfield_10021";
pub const SCOPES: [&str; 4] = ["read:jira-work", "write:jira-work", "read:jira-user", "offline_access"];
const RESOURCES: &str = "https://api.atlassian.com/oauth/token/accessible-resources";
const LOGO: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64"><rect width="64" height="64" rx="12" fill="#0052CC"/><path d="M32 12l14 14-14 14-14-14z" fill="#fff" opacity=".55"/><path d="M32 24l14 14-14 14-14-14z" fill="#fff"/></svg>"##;

#[derive(Debug, Clone, Copy, Default)]
pub struct Jira;

pub fn oauth_app() -> OAuthApp {
    OAuthApp {
        client_id: brand::connectors::ATLASSIAN_CLIENT_ID.to_owned(),
        client_secret: Some(brand::connectors::ATLASSIAN_CLIENT_SECRET.to_owned()),
        redirect: brand::connectors::OAUTH_REDIRECT.to_owned(),
        authorize_endpoint: "https://auth.atlassian.com/authorize".to_owned(),
        token_endpoint: "https://auth.atlassian.com/oauth/token".to_owned(),
        scopes: SCOPES.iter().map(|scope| (*scope).to_owned()).collect(),
        extra_authorize: vec![
            ("audience".to_owned(), "api.atlassian.com".to_owned()),
            ("prompt".to_owned(), "consent".to_owned()),
        ],
        token_body: TokenBody::Json,
    }
}

#[derive(Debug)]
struct Api<'a> {
    http: &'a dyn Http,
    base: String,
    site: String,
    token: &'a str,
    flag_field: String,
    project: Option<String>,
}

impl<'a> Api<'a> {
    fn new(http: &'a dyn Http, creds: &'a Credentials) -> Result<Api<'a>, ActionError> {
        let cloud_id =
            creds.setting("cloud_id").ok_or_else(|| ActionError::permanent("the connection has no site chosen"))?;
        Ok(Api {
            http,
            base: format!("https://api.atlassian.com/ex/jira/{cloud_id}/rest/api/3"),
            site: creds.setting("site_url").unwrap_or_default().trim_end_matches('/').to_owned(),
            token: &creds.access_token,
            flag_field: creds.setting("flag_field").unwrap_or(DEFAULT_FLAG_FIELD).to_owned(),
            project: creds.setting("project").map(str::to_owned),
        })
    }

    fn request(&self, method: Method, path: &str) -> HttpRequest {
        HttpRequest::new(method, format!("{}{path}", self.base)).bearer(self.token).header("accept", "application/json")
    }

    async fn get(&self, path: &str) -> Result<Value, ActionError> {
        fetch_json(self.http, self.request(Method::Get, path)).await
    }

    async fn send(&self, method: Method, path: &str, body: &Value) -> Result<Value, ActionError> {
        fetch(self.http, self.request(method, path).json(body)?).await?.json()
    }

    fn browse(&self, key: &str) -> String {
        if self.site.is_empty() { String::new() } else { format!("{}/browse/{key}", self.site) }
    }
}

pub fn jql_text(text: &str) -> String {
    text.chars()
        .map(|c| if c.is_alphanumeric() || c == ' ' { c } else { ' ' })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn jql_key(key: &str) -> Result<&str, ActionError> {
    let valid = !key.is_empty() && key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
    if valid { Ok(key) } else { Err(ActionError::permanent(format!("{key:?} is not a project or issue key"))) }
}

#[async_trait]
impl Provider for Jira {
    fn descriptor(&self) -> ProviderDescriptor {
        ProviderDescriptor {
            key: ProviderKey::from(KEY),
            name: "Jira".to_owned(),
            logo: LOGO.to_owned(),
            capabilities: vec![Capability::from(contracts::TRACKER)],
            auth: AuthSpec::Redirect(oauth_app()),
            actions: contracts::tracker(),
        }
    }

    async fn call(
        &self,
        http: &dyn Http,
        creds: &Credentials,
        action: &str,
        args: &Value,
        idempotency_key: &str,
    ) -> Result<Value, ActionError> {
        let api = Api::new(http, creds)?;
        let args = Args(args);
        match action {
            actions::CREATE_ISSUE => tracker::create_issue(&api, args, idempotency_key).await,
            actions::ISSUE => tracker::issue(&api, args.text("issue")?).await,
            actions::COMMENT => tracker::comment(&api, args.text("issue")?, args.text("body")?, idempotency_key).await,
            actions::STATUSES => tracker::statuses(&api, args).await,
            actions::TRANSITION => tracker::transition(&api, args.text("issue")?, args.text("status")?).await,
            actions::SET_IMPEDIMENT => tracker::set_impediment(&api, args, idempotency_key).await,
            actions::SEARCH => tracker::search(&api, args).await,
            _ => Err(unknown_action(action)),
        }
    }

    async fn accounts(&self, http: &dyn Http, creds: &Credentials) -> Result<Vec<AccountOption>, ActionError> {
        let request = HttpRequest::get(RESOURCES).bearer(&creds.access_token).header("accept", "application/json");
        let sites = fetch_json(http, request).await?;
        Ok(sites
            .as_array()
            .map(Vec::as_slice)
            .unwrap_or_default()
            .iter()
            .map(|site| {
                let id = text_of(site, "/id");
                let url = text_of(site, "/url");
                AccountOption {
                    label: format!("{} ({url})", text_of(site, "/name")),
                    settings: json!({ "cloud_id": id, "site_url": url }),
                    id,
                }
            })
            .collect())
    }

    fn events(&self) -> Option<&dyn EventSource> {
        Some(&events::JiraEvents)
    }
}

#[cfg(test)]
mod tests;
