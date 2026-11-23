use async_trait::async_trait;
use domain::connection::{Capability, ProviderKey};
use serde_json::{Value, json};

use crate::contracts::{self, cloud};
use crate::descriptor::{AuthSpec, ProviderDescriptor};
use crate::error::ActionError;
use crate::http::{Http, HttpRequest, fetch_json};
use crate::oauth::{DeviceApp, Dialect, SsoSpec};
use crate::provider::{AccountOption, Args, Credentials, Provider, text_of, unknown_action};

pub const KEY: &str = "aws";
const SCOPE: &str = "sso:account:access";
const PAGES: usize = 20;
const LOGO: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64"><rect width="64" height="64" rx="12" fill="#232F3E"/><path d="M14 40c10 7 26 7 36 0" stroke="#FF9900" stroke-width="4" fill="none" stroke-linecap="round"/><path d="M45 36l6 3-3 6" stroke="#FF9900" stroke-width="4" fill="none" stroke-linecap="round"/><text x="32" y="32" font-family="Arial" font-weight="700" font-size="17" fill="#fff" text-anchor="middle">aws</text></svg>"##;

#[derive(Debug, Clone, Copy, Default)]
pub struct Aws;

fn region(creds: Option<&Credentials>) -> String {
    creds.and_then(|creds| creds.setting("region")).unwrap_or(brand::connectors::AWS_SSO_REGION).to_owned()
}

pub fn oidc(region: &str) -> String {
    format!("https://oidc.{region}.amazonaws.com")
}

pub fn portal(region: &str) -> String {
    format!("https://portal.sso.{region}.amazonaws.com")
}

pub fn sso_spec(region: &str) -> SsoSpec {
    let base = oidc(region);
    SsoSpec {
        registration_endpoint: format!("{base}/client/register"),
        client_name: brand::NAME.to_owned(),
        scopes: vec![SCOPE.to_owned()],
        start_field: "start_url".to_owned(),
        device: DeviceApp {
            client_id: String::new(),
            client_secret: None,
            device_endpoint: format!("{base}/device_authorization"),
            token_endpoint: format!("{base}/token"),
            scopes: Vec::new(),
            start_fields: Vec::new(),
            dialect: Dialect::CamelJson,
        },
    }
}

async fn portal_get(
    http: &dyn Http,
    creds: &Credentials,
    path: &str,
    query: &[(&str, &str)],
) -> Result<Value, ActionError> {
    let mut url = url::Url::parse(&format!("{}{path}", portal(&region(Some(creds)))))
        .map_err(|error| ActionError::permanent(error.to_string()))?;
    url.query_pairs_mut().extend_pairs(query);
    let request = HttpRequest::get(String::from(url)).header("x-amz-sso_bearer_token", creds.access_token.clone());
    fetch_json(http, request).await
}

async fn paged(
    http: &dyn Http,
    creds: &Credentials,
    path: &str,
    query: &[(&str, &str)],
    field: &str,
) -> Result<Vec<Value>, ActionError> {
    let mut found = Vec::new();
    let mut token: Option<String> = None;
    for _ in 0..PAGES {
        let mut pairs = query.to_vec();
        pairs.push(("max_result", "100"));
        if let Some(next) = token.as_deref() {
            pairs.push(("next_token", next));
        }
        let page = portal_get(http, creds, path, &pairs).await?;
        found.extend(page[field].as_array().cloned().unwrap_or_default());
        token = page["nextToken"].as_str().filter(|next| !next.is_empty()).map(str::to_owned);
        if token.is_none() {
            break;
        }
    }
    Ok(found)
}

async fn accounts_and_roles(http: &dyn Http, creds: &Credentials) -> Result<Value, ActionError> {
    let accounts = paged(http, creds, "/assignment/accounts", &[], "accountList").await?;
    let mut listed = Vec::new();
    for account in accounts {
        let id = text_of(&account, "/accountId");
        let roles = paged(http, creds, "/assignment/roles", &[("account_id", &id)], "roleList").await?;
        let roles: Vec<String> = roles.iter().map(|role| text_of(role, "/roleName")).collect();
        listed.push(json!({
            "id": id,
            "name": text_of(&account, "/accountName"),
            "email": text_of(&account, "/emailAddress"),
            "roles": roles,
        }));
    }
    Ok(json!({ "accounts": listed }))
}

async fn identity(http: &dyn Http, creds: &Credentials, args: Args<'_>) -> Result<Value, ActionError> {
    let account = args.text("account")?;
    let role = args.text("role")?;
    let answer =
        portal_get(http, creds, "/federation/credentials", &[("account_id", account), ("role_name", role)]).await?;
    let held = answer.get("roleCredentials").ok_or_else(|| ActionError::decode("no roleCredentials"))?;
    let expires_at = held["expiration"].as_i64().and_then(|millis| jiff::Timestamp::from_millisecond(millis).ok());
    Ok(json!({
        "account": account,
        "role": role,
        "region": region(Some(creds)),
        "access_key_id": text_of(held, "/accessKeyId"),
        "secret_access_key": text_of(held, "/secretAccessKey"),
        "session_token": text_of(held, "/sessionToken"),
        "expires_at": expires_at.map(|at| at.to_string()),
    }))
}

#[async_trait]
impl Provider for Aws {
    fn descriptor(&self) -> ProviderDescriptor {
        ProviderDescriptor {
            key: ProviderKey::from(KEY),
            name: "AWS".to_owned(),
            logo: LOGO.to_owned(),
            capabilities: vec![Capability::from(contracts::CLOUD)],
            auth: AuthSpec::Sso(sso_spec(&region(None))),
            actions: contracts::cloud(),
        }
    }

    async fn call(
        &self,
        http: &dyn Http,
        creds: &Credentials,
        action: &str,
        args: &Value,
        _idempotency_key: &str,
    ) -> Result<Value, ActionError> {
        match action {
            cloud::ACCOUNTS_AND_ROLES => accounts_and_roles(http, creds).await,
            cloud::IDENTITY => identity(http, creds, Args(args)).await,
            _ => Err(unknown_action(action)),
        }
    }

    async fn accounts(&self, http: &dyn Http, creds: &Credentials) -> Result<Vec<AccountOption>, ActionError> {
        let accounts = paged(http, creds, "/assignment/accounts", &[], "accountList").await?;
        Ok(accounts
            .iter()
            .map(|account| {
                let id = text_of(account, "/accountId");
                AccountOption {
                    label: format!("{} ({id})", text_of(account, "/accountName")),
                    settings: json!({ "account": id, "region": region(Some(creds)) }),
                    id,
                }
            })
            .collect())
    }
}

#[cfg(test)]
mod tests;
