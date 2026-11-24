mod login;
mod sts;

use async_trait::async_trait;
use domain::connection::{Capability, ProviderKey};
use serde_json::{Value, json};

use crate::contracts::{self, cloud};
use crate::descriptor::{AlternateAuth, AuthSpec, Presentation, ProviderDescriptor, TokenField, TokenForm};
use crate::error::ActionError;
use crate::http::{Http, HttpRequest, fetch_json};
use crate::oauth::{DeviceApp, Dialect, Loopback, SsoSpec};
use crate::provider::{AccountOption, Args, Credentials, Provider, text_of, unknown_action};

pub use login::ConsoleLogin;
pub use sts::{Keys, signature};

pub const KEY: &str = "aws";
pub const SSO: &str = "sso";
pub const ACCESS_KEYS: &str = "access_keys";
const SCOPE: &str = "sso:account:access";
const PAGES: usize = 20;
const LOGO: &str = include_str!("logo.svg");

#[derive(Debug, Clone, Copy, Default)]
pub struct Aws;

fn region(creds: Option<&Credentials>) -> String {
    creds.and_then(|creds| creds.setting("region")).unwrap_or(brand::connectors::AWS_REGION).to_owned()
}

pub fn portal(region: &str) -> String {
    format!("https://portal.sso.{region}.amazonaws.com")
}

pub fn access_keys() -> AlternateAuth {
    let id = TokenField::new("accessKeyId", "Access key ID", "", false);
    let help = "No IAM, abra o usuário, Security credentials, e crie uma access key.";
    let secret = TokenField::new("secretAccessKey", "Secret access key", help, true);
    AlternateAuth::token(ACCESS_KEYS, "Usar access key", "da AWS", vec![id, secret], TokenForm::Json)
}

pub fn sso_spec() -> SsoSpec {
    let base = "https://oidc.{region}.amazonaws.com";
    SsoSpec {
        registration_endpoint: format!("{base}/client/register"),
        client_name: brand::NAME.to_owned(),
        scopes: vec![SCOPE.to_owned()],
        start_field: "start_url".to_owned(),
        region: brand::connectors::AWS_REGION.to_owned(),
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

async fn sso_accounts(http: &dyn Http, creds: &Credentials) -> Result<Vec<Value>, ActionError> {
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
    Ok(listed)
}

async fn accounts_and_roles(http: &dyn Http, creds: &Credentials) -> Result<Value, ActionError> {
    if let Some(keys) = Keys::read(&creds.access_token) {
        let caller = sts::caller(http, &keys, &region(Some(creds))).await?;
        let account = json!({ "id": caller.account, "name": caller.arn, "email": "", "roles": [] });
        return Ok(json!({ "accounts": [account] }));
    }
    Ok(json!({ "accounts": sso_accounts(http, creds).await? }))
}

async fn console_identity(http: &dyn Http, creds: &Credentials, keys: &Keys) -> Result<Value, ActionError> {
    let place = region(Some(creds));
    let caller = sts::caller(http, keys, &place).await?;
    Ok(json!({
        "user": caller.arn,
        "arn": caller.arn,
        "account": caller.account,
        "role": Value::Null,
        "region": place,
        "access_key_id": keys.access_key_id,
        "secret_access_key": keys.secret_access_key,
        "session_token": keys.session_token,
        "expires_at": keys.expires_at,
    }))
}

async fn identity(http: &dyn Http, creds: &Credentials, args: Args<'_>) -> Result<Value, ActionError> {
    if let Some(keys) = Keys::read(&creds.access_token) {
        return console_identity(http, creds, &keys).await;
    }
    let account =
        args.optional_text("account").or(creds.setting("account")).ok_or_else(|| ActionError::missing("account"))?;
    let role = args.optional_text("role").or(creds.setting("role")).ok_or_else(|| ActionError::missing("role"))?;
    let answer =
        portal_get(http, creds, "/federation/credentials", &[("account_id", account), ("role_name", role)]).await?;
    let held = answer.get("roleCredentials").ok_or_else(|| ActionError::decode("no roleCredentials"))?;
    let expires_at = held["expiration"].as_i64().and_then(|millis| jiff::Timestamp::from_millisecond(millis).ok());
    Ok(json!({
        "user": role,
        "account": account,
        "role": role,
        "region": region(Some(creds)),
        "access_key_id": text_of(held, "/accessKeyId"),
        "secret_access_key": text_of(held, "/secretAccessKey"),
        "session_token": text_of(held, "/sessionToken"),
        "expires_at": expires_at.map(|at| at.to_string()),
    }))
}

pub fn spaced(account: &str) -> String {
    if account.len() != 12 || !account.chars().all(|c| c.is_ascii_digit()) {
        return account.to_owned();
    }
    format!("{}-{}-{}", &account[..4], &account[4..8], &account[8..])
}

fn sso_options(listed: &[Value], place: &str) -> Vec<AccountOption> {
    let mut options = Vec::new();
    for account in listed {
        let id = text_of(account, "/id");
        let label = format!("{} · {}", text_of(account, "/name"), spaced(&id));
        for role in account["roles"].as_array().map(Vec::as_slice).unwrap_or_default() {
            let role = role.as_str().unwrap_or_default();
            options.push(AccountOption {
                id: format!("{id}/{role}"),
                label: label.clone(),
                settings: json!({ "account": id, "role": role, "region": place, "detail": role }),
            });
        }
    }
    options
}

#[async_trait]
impl Provider for Aws {
    fn descriptor(&self) -> ProviderDescriptor {
        ProviderDescriptor {
            key: ProviderKey::from(KEY),
            name: "AWS".to_owned(),
            logo: LOGO.to_owned(),
            capabilities: vec![Capability::from(contracts::CLOUD)],
            auth: AuthSpec::Loopback(Loopback::new(ConsoleLogin { region: brand::connectors::AWS_REGION.to_owned() })),
            actions: contracts::cloud(),
        }
    }

    fn presentation(&self) -> Presentation {
        Presentation {
            description: "Sua conta AWS".to_owned(),
            group: "Nuvem".to_owned(),
            button: "Entrar com AWS".to_owned(),
            site: "da AWS".to_owned(),
            choose_title: "Conta e papel".to_owned(),
        }
    }

    fn alternatives(&self) -> Vec<AlternateAuth> {
        let sso = AlternateAuth {
            key: SSO.to_owned(),
            label: "Usar SSO da empresa".to_owned(),
            button: "Continuar".to_owned(),
            site: "do AWS IAM Identity Center".to_owned(),
            auth: AuthSpec::Sso(sso_spec()),
        };
        vec![sso, access_keys()]
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
        let place = region(Some(creds));
        if let Some(keys) = Keys::read(&creds.access_token) {
            let caller = sts::caller(http, &keys, &place).await?;
            let name = caller.arn.rsplit([':', '/']).next().unwrap_or_default().to_owned();
            return Ok(vec![AccountOption {
                id: caller.account.clone(),
                label: format!("{name} · {}", spaced(&caller.account)),
                settings: json!({ "account": caller.account, "arn": caller.arn, "region": place, "detail": caller.arn }),
            }]);
        }
        Ok(sso_options(&sso_accounts(http, creds).await?, &place))
    }
}

#[cfg(test)]
mod tests;
