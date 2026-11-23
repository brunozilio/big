mod adf;
mod events;
mod tracker;

use async_trait::async_trait;
use domain::connection::{Capability, ProviderKey};
use serde_json::{Map, Value, json};

use crate::contracts::{self, tracker as actions};
use crate::descriptor::{AuthSpec, Presentation, ProviderDescriptor};
use crate::error::ActionError;
use crate::http::Http;
use crate::mcp::{self, McpClient};
use crate::oauth::{Loopback, mcp::McpSignIn};
use crate::provider::{AccountOption, Args, Credentials, EventSource, Provider, text_of, unknown_action};

pub const KEY: &str = "jira";
pub const DEFAULT_FLAG_FIELD: &str = "customfield_10021";
pub const SCOPES: [&str; 6] = [
    "read:jira:agent-interface",
    "write:jira:agent-interface",
    "search:jira:agent-interface",
    "read:me",
    "read:account",
    "offline_access",
];
const LOGO: &str = include_str!("logo.svg");
const PAGE: u64 = 50;
const MOST_PAGES: u64 = 10;
pub const NO_SITE: &str = "Reconecte o Jira e escolha o site.";
pub const NO_PROJECT: &str = "Escolha o projeto.";
pub const NO_SITES: &str =
    "Nenhum site do Jira foi encontrado nesta conta Atlassian. Confira se a conta tem acesso ao Jira e conecte de novo.";

#[derive(Debug, Clone, Copy, Default)]
pub struct Jira;

pub fn sign_in() -> McpSignIn {
    McpSignIn {
        server: brand::connectors::ATLASSIAN_MCP_URL.to_owned(),
        redirect: brand::connectors::OAUTH_REDIRECT.to_owned(),
        client_name: brand::NAME.to_owned(),
        scopes: SCOPES.iter().map(|scope| (*scope).to_owned()).collect(),
    }
}

#[derive(Debug)]
struct Api<'a> {
    mcp: McpClient<'a>,
    cloud: String,
    site: String,
    flag_field: String,
    project: Option<String>,
}

impl<'a> Api<'a> {
    async fn new(http: &'a dyn Http, creds: &'a Credentials) -> Result<Api<'a>, ActionError> {
        let cloud = creds.setting("cloud_id").ok_or_else(|| ActionError::blocked(NO_SITE))?;
        Ok(Api {
            mcp: McpClient::connect(http, brand::connectors::ATLASSIAN_MCP_URL, &creds.access_token).await?,
            cloud: cloud.to_owned(),
            site: creds.setting("site_url").unwrap_or_default().trim_end_matches('/').to_owned(),
            flag_field: creds.setting("flag_field").unwrap_or(DEFAULT_FLAG_FIELD).to_owned(),
            project: creds.setting("project").map(str::to_owned),
        })
    }

    async fn tool(&self, name: &str, arguments: Value) -> Result<Value, ActionError> {
        let mut arguments: Map<String, Value> = arguments.as_object().cloned().unwrap_or_default();
        arguments.insert("cloudId".to_owned(), json!(self.cloud));
        self.mcp.call(name, Value::Object(arguments)).await
    }

    async fn search(&self, jql: &str, fields: &[&str], token: Option<&str>) -> Result<Value, ActionError> {
        let mut arguments = json!({
            "jql": jql, "fields": fields, "maxResults": 100, "responseContentFormat": "markdown",
        });
        if let Some(token) = token {
            arguments["nextPageToken"] = json!(token);
        }
        self.tool("searchJiraIssuesUsingJql", arguments).await
    }

    fn project<'p>(&'p self, args: Args<'p>) -> Result<&'p str, ActionError> {
        let project = args.optional_text("project").or(self.project.as_deref());
        jql_key(project.ok_or_else(|| ActionError::blocked(NO_PROJECT))?)
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

pub fn listed(value: &Value, field: &str) -> Vec<Value> {
    mcp::items(value, field)
}

pub fn project_of(project: &Value) -> Value {
    let avatar = project.pointer("/avatarUrls/48x48").and_then(Value::as_str);
    json!({
        "id": text_of(project, "/id"),
        "key": text_of(project, "/key"),
        "name": text_of(project, "/name"),
        "avatar": avatar,
    })
}

pub async fn visible_projects(mcp: &McpClient<'_>, cloud: &str, query: Option<&str>) -> Result<Vec<Value>, ActionError> {
    let mut found = Vec::new();
    for page in 0..MOST_PAGES {
        let mut arguments = json!({
            "cloudId": cloud, "action": "browse", "expandIssueTypes": false, "maxResults": PAGE, "startAt": page * PAGE,
        });
        if let Some(query) = query.map(str::trim).filter(|query| !query.is_empty()) {
            arguments["searchString"] = json!(query);
        }
        let answer = mcp.call("getVisibleJiraProjects", arguments).await?;
        let projects = listed(&answer, "values");
        let fetched = projects.len();
        found.extend(projects.iter().map(project_of).filter(|project| !text_of(project, "/key").is_empty()));
        if answer["isLast"] == json!(true) || u64::try_from(fetched).unwrap_or(0) < PAGE {
            break;
        }
    }
    Ok(found)
}

fn site_settings(site: &Value, projects: &[Value]) -> Value {
    let keys: Vec<String> = projects.iter().map(|project| text_of(project, "/key")).collect();
    let detail = match keys.len() {
        0 => text_of(site, "/name"),
        1 => format!("Projeto {}", keys[0]),
        _ => format!("Projetos {}", keys.join(", ")),
    };
    let url = text_of(site, "/url");
    let mut settings = json!({ "cloud_id": text_of(site, "/id"), "site_url": url, "detail": detail });
    if let [only] = keys.as_slice() {
        settings["project"] = json!(only);
    }
    settings
}

fn site_label(site: &Value) -> String {
    let url = text_of(site, "/url");
    let label = url.trim_start_matches("https://").trim_end_matches('/');
    if label.is_empty() { text_of(site, "/name") } else { label.to_owned() }
}

#[async_trait]
impl Provider for Jira {
    fn descriptor(&self) -> ProviderDescriptor {
        ProviderDescriptor {
            key: ProviderKey::from(KEY),
            name: "Jira".to_owned(),
            logo: LOGO.to_owned(),
            capabilities: vec![Capability::from(contracts::TRACKER)],
            auth: AuthSpec::Loopback(Loopback::new(sign_in())),
            actions: contracts::tracker(),
        }
    }

    fn presentation(&self) -> Presentation {
        Presentation {
            description: "Sua conta Atlassian".to_owned(),
            group: "Tarefas".to_owned(),
            button: "Continuar na Atlassian".to_owned(),
            site: "da Atlassian".to_owned(),
            choose_title: "Site do Jira".to_owned(),
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
        let args = Args(args);
        let api = Api::new(http, creds).await?;
        match action {
            actions::CREATE_ISSUE => tracker::create_issue(&api, args, idempotency_key).await,
            actions::ISSUE => tracker::issue(&api, args.text("issue")?).await,
            actions::COMMENT => tracker::comment(&api, args.text("issue")?, args.text("body")?, idempotency_key).await,
            actions::STATUSES => tracker::statuses(&api, api.project(args)?).await,
            actions::TRANSITION => tracker::transition(&api, args.text("issue")?, args.text("status")?).await,
            actions::SET_IMPEDIMENT => tracker::set_impediment(&api, args, idempotency_key).await,
            actions::SEARCH => tracker::search(&api, args).await,
            actions::PROJECTS => tracker::projects(&api, args).await,
            _ => Err(unknown_action(action)),
        }
    }

    async fn accounts(&self, http: &dyn Http, creds: &Credentials) -> Result<Vec<AccountOption>, ActionError> {
        let mcp = McpClient::connect(http, brand::connectors::ATLASSIAN_MCP_URL, &creds.access_token).await?;
        let sites = mcp.call("getAccessibleAtlassianResources", json!({})).await?;
        let sites: Vec<Value> =
            listed(&sites, "resources").into_iter().filter(|site| !text_of(site, "/id").is_empty()).collect();
        if sites.is_empty() {
            return Err(ActionError::permanent(NO_SITES));
        }
        let mut options = Vec::new();
        for site in &sites {
            let id = text_of(site, "/id");
            let projects = visible_projects(&mcp, &id, None).await.unwrap_or_default();
            options.push(AccountOption { label: site_label(site), settings: site_settings(site, &projects), id });
        }
        Ok(options)
    }

    fn chooses(&self) -> &'static [&'static str] {
        &["cloud_id"]
    }

    fn events(&self) -> Option<&dyn EventSource> {
        Some(&events::JiraEvents)
    }
}

#[cfg(test)]
mod tests;
