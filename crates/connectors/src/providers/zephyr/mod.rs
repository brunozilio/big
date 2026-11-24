mod cases;
#[cfg(test)]
mod fake;

use async_trait::async_trait;
use domain::connection::{Capability, ProviderKey};
use serde_json::{Value, json};

use crate::contracts::{self, tests as actions};
use crate::descriptor::{AlternateAuth, AuthSpec, Presentation, ProviderDescriptor, TokenField, TokenForm};
use crate::error::ActionError;
use crate::http::Http;
use crate::mcp::{self, McpClient};
use crate::oauth::{Loopback, mcp::McpSignIn};
use crate::provider::{AccountOption, Args, Credentials, Provider, text_of, unknown_action};

pub const KEY: &str = "zephyr";
pub const SCOPES: [&str; 1] = ["api"];
const LOGO: &str = include_str!("logo.svg");
const PAGE: u64 = 50;
const MOST_PAGES: u64 = 10;
pub const NO_PROJECT: &str = "Escolha o projeto.";
pub const API_TOKEN: &str = "api_token";

#[derive(Debug, Clone, Copy, Default)]
pub struct Zephyr;

pub fn api_token() -> AlternateAuth {
    let help =
        "No Jira, abra ⚙ → Configurações gerais → Apps → Zephyr API Access Tokens e clique em Gerar token de acesso.";
    let token = TokenField::new("token", "API token do Zephyr", help, true);
    AlternateAuth::token(API_TOKEN, "Usar API token", "do Zephyr", vec![token], TokenForm::Bearer)
}

pub fn sign_in() -> McpSignIn {
    McpSignIn {
        server: brand::connectors::ZEPHYR_MCP_URL.to_owned(),
        redirect: brand::connectors::ZEPHYR_OAUTH_REDIRECT.to_owned(),
        client_name: brand::NAME.to_owned(),
        scopes: SCOPES.iter().map(|scope| (*scope).to_owned()).collect(),
    }
}

#[derive(Debug)]
struct Api<'a> {
    mcp: McpClient<'a>,
    project: Option<String>,
}

impl<'a> Api<'a> {
    async fn new(http: &'a dyn Http, creds: &'a Credentials) -> Result<Api<'a>, ActionError> {
        Ok(Api {
            mcp: McpClient::connect(http, brand::connectors::ZEPHYR_MCP_URL, &creds.access_token).await?,
            project: creds.setting("project").map(str::to_owned),
        })
    }

    async fn tool(&self, name: &str, arguments: Value) -> Result<Value, ActionError> {
        self.mcp.call(name, arguments).await
    }

    fn project<'p>(&'p self, args: Args<'p>) -> Result<&'p str, ActionError> {
        let from_issue =
            args.optional_text("issue").and_then(|issue| issue.rsplit_once('-')).map(|(project, _)| project);
        let project =
            args.optional_text("project").or(self.project.as_deref()).or(from_issue.filter(|key| !key.is_empty()));
        valid_key(project.ok_or_else(|| ActionError::blocked(NO_PROJECT))?)
    }
}

pub fn valid_key(key: &str) -> Result<&str, ActionError> {
    let valid = !key.is_empty() && key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
    if valid { Ok(key) } else { Err(ActionError::permanent(format!("{key:?} is not a project or test key"))) }
}

fn project_of(project: &Value) -> Value {
    let key = text_of(project, "/key");
    let name = text_of(project, "/name");
    json!({
        "id": cases::id_of(project, "/id"),
        "name": if name.is_empty() { key.clone() } else { name },
        "key": key,
    })
}

fn last_page(page: &Value, fetched: usize) -> bool {
    page["isLast"] == json!(true) || u64::try_from(fetched).unwrap_or(0) < PAGE
}

async fn all_projects(mcp: &McpClient<'_>) -> Result<Vec<Value>, ActionError> {
    let mut found = Vec::new();
    for page in 0..MOST_PAGES {
        let answer = mcp.call("zephyr_get_projects", json!({ "startAt": page * PAGE, "maxResults": PAGE })).await?;
        let projects = mcp::items(&answer, "values");
        found.extend(projects.iter().map(project_of).filter(|project| !text_of(project, "/key").is_empty()));
        if last_page(&answer, projects.len()) {
            break;
        }
    }
    Ok(found)
}

async fn projects(api: &Api<'_>, args: Args<'_>) -> Result<Value, ActionError> {
    let query = args.optional_text("query").map(|query| query.trim().to_lowercase()).unwrap_or_default();
    let matches = |project: &Value| {
        query.is_empty()
            || text_of(project, "/key").to_lowercase().contains(&query)
            || text_of(project, "/name").to_lowercase().contains(&query)
    };
    Ok(Value::Array(all_projects(&api.mcp).await?.into_iter().filter(matches).collect()))
}

fn account_settings(projects: &[Value]) -> Value {
    let keys: Vec<String> = projects.iter().map(|project| text_of(project, "/key")).collect();
    let detail = match keys.len() {
        0 => "Nenhum projeto".to_owned(),
        1 => format!("Projeto {}", keys[0]),
        _ => format!("Projetos {}", keys.join(", ")),
    };
    let mut settings = json!({ "detail": detail });
    if let [only] = keys.as_slice() {
        settings["project"] = json!(only);
    }
    settings
}

#[async_trait]
impl Provider for Zephyr {
    fn descriptor(&self) -> ProviderDescriptor {
        ProviderDescriptor {
            key: ProviderKey::from(KEY),
            name: "Zephyr".to_owned(),
            logo: LOGO.to_owned(),
            capabilities: vec![Capability::from(contracts::TESTS)],
            auth: AuthSpec::Loopback(Loopback::new(sign_in())),
            actions: contracts::tests(),
        }
    }

    fn presentation(&self) -> Presentation {
        Presentation {
            description: "Sua conta Zephyr Scale".to_owned(),
            group: "Testes".to_owned(),
            button: "Continuar na SmartBear".to_owned(),
            site: "da SmartBear".to_owned(),
            choose_title: "Conta do Zephyr".to_owned(),
        }
    }

    fn alternatives(&self) -> Vec<AlternateAuth> {
        vec![api_token()]
    }

    async fn call(
        &self,
        http: &dyn Http,
        creds: &Credentials,
        action: &str,
        args: &Value,
        idempotency_key: &str,
    ) -> Result<Value, ActionError> {
        let args = Args(args);
        let api = Api::new(http, creds).await?;
        match action {
            actions::CREATE_CASE => cases::create_case(&api, args, idempotency_key).await,
            actions::CASES => cases::cases(&api, args.text("issue")?).await,
            actions::CREATE_CYCLE => cases::create_cycle(&api, args, idempotency_key).await,
            actions::RECORD => cases::record(&api, args, idempotency_key).await,
            actions::PROJECTS => projects(&api, args).await,
            _ => Err(unknown_action(action)),
        }
    }

    async fn accounts(&self, http: &dyn Http, creds: &Credentials) -> Result<Vec<AccountOption>, ActionError> {
        let mcp = McpClient::connect(http, brand::connectors::ZEPHYR_MCP_URL, &creds.access_token).await?;
        let projects = all_projects(&mcp).await?;
        Ok(vec![AccountOption {
            id: KEY.to_owned(),
            label: "Zephyr Scale".to_owned(),
            settings: account_settings(&projects),
        }])
    }
}

#[cfg(test)]
mod tests;
