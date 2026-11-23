mod code;
mod events;
mod pipelines;
mod workflow;

use async_trait::async_trait;
use domain::connection::{Capability, ProviderKey};
use serde_json::{Value, json};

use crate::contracts::{self, code as code_actions, pipelines as pipeline_actions};
use crate::descriptor::{AuthSpec, Presentation, ProviderDescriptor};
use crate::error::ActionError;
use crate::http::{Http, HttpRequest, HttpResponse, Method, fetch};
use crate::oauth::{DeviceApp, Dialect};
use crate::provider::{AccountOption, Args, Credentials, EventSource, Provider, text_of, unknown_action};

pub const KEY: &str = "github";
pub const API: &str = "https://api.github.com";
pub const SCOPES: [&str; 3] = ["repo", "read:org", "workflow"];
const API_VERSION: &str = "2022-11-28";
const LOGO: &str = include_str!("logo.svg");

#[derive(Debug, Clone, Copy, Default)]
pub struct Github;

pub fn device_app() -> DeviceApp {
    DeviceApp {
        client_id: brand::connectors::GITHUB_CLIENT_ID.to_owned(),
        client_secret: None,
        device_endpoint: "https://github.com/login/device/code".to_owned(),
        token_endpoint: "https://github.com/login/oauth/access_token".to_owned(),
        scopes: SCOPES.iter().map(|scope| (*scope).to_owned()).collect(),
        start_fields: Vec::new(),
        dialect: Dialect::Form,
    }
}

#[derive(Debug)]
struct Api<'a> {
    http: &'a dyn Http,
    base: String,
    token: &'a str,
}

impl<'a> Api<'a> {
    fn new(http: &'a dyn Http, creds: &'a Credentials) -> Api<'a> {
        let base = creds.setting("api").unwrap_or(API).trim_end_matches('/').to_owned();
        Api { http, base, token: &creds.access_token }
    }

    fn request(&self, method: Method, path: &str) -> HttpRequest {
        HttpRequest::new(method, format!("{}{path}", self.base))
            .bearer(self.token)
            .header("accept", "application/vnd.github+json")
            .header("x-github-api-version", API_VERSION)
    }

    async fn raw(&self, request: HttpRequest) -> Result<HttpResponse, ActionError> {
        fetch(self.http, request).await
    }

    async fn get(&self, path: &str) -> Result<Value, ActionError> {
        self.raw(self.request(Method::Get, path)).await?.json()
    }

    async fn send(&self, method: Method, path: &str, body: &Value) -> Result<Value, ActionError> {
        self.raw(self.request(method, path).json(body)?).await?.json()
    }

    async fn graphql(&self, query: &str, variables: Value) -> Result<Value, ActionError> {
        let answer = self.send(Method::Post, "/graphql", &json!({ "query": query, "variables": variables })).await?;
        match answer["errors"].as_array().and_then(|errors| errors.first()) {
            Some(error) => Err(ActionError::permanent(text_of(error, "/message"))),
            None => Ok(answer["data"].clone()),
        }
    }
}

pub fn repo(args: Args<'_>) -> Result<&str, ActionError> {
    let repo = args.text("repo")?;
    let part = |part: &str| {
        !part.is_empty()
            && part != "."
            && part != ".."
            && part.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
    };
    match repo.split_once('/') {
        Some((owner, name)) if part(owner) && part(name) => Ok(repo),
        _ => Err(ActionError::permanent(format!("{repo:?} is not owner/name"))),
    }
}

#[async_trait]
impl Provider for Github {
    fn descriptor(&self) -> ProviderDescriptor {
        ProviderDescriptor {
            key: ProviderKey::from(KEY),
            name: "GitHub".to_owned(),
            logo: LOGO.to_owned(),
            capabilities: vec![Capability::from(contracts::CODE), Capability::from(contracts::PIPELINES)],
            auth: AuthSpec::Device(device_app()),
            actions: [contracts::code(), contracts::pipelines()].concat(),
        }
    }

    fn presentation(&self) -> Presentation {
        Presentation {
            description: "Sua conta GitHub".to_owned(),
            group: "Código".to_owned(),
            button: "Entrar com GitHub".to_owned(),
            site: "do GitHub".to_owned(),
            choose_title: "Organização".to_owned(),
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
        let api = Api::new(http, creds);
        let args = Args(args);
        match action {
            code_actions::OPEN_PULL_REQUEST => code::open_pull_request(&api, args, idempotency_key).await,
            code_actions::PULL_REQUEST => code::pull_request(&api, args).await,
            code_actions::FILES_DIFF => code::files_diff(&api, args).await,
            code_actions::REVIEW_COMMENTS => code::review_comments(&api, args).await,
            code_actions::SUBMIT_REVIEW => code::submit_review(&api, args, idempotency_key).await,
            code_actions::RESOLVE_THREAD => code::resolve_thread(&api, args).await,
            code_actions::MERGE => code::merge(&api, args).await,
            pipeline_actions::RUNS => pipelines::runs(&api, args).await,
            pipeline_actions::RUN => pipelines::run(&api, args).await,
            pipeline_actions::JOB_LOG => pipelines::job_log(&api, args).await,
            pipeline_actions::PENDING_DEPLOYMENTS => pipelines::pending_deployments(&api, args).await,
            pipeline_actions::REVIEW_DEPLOYMENT => pipelines::review_deployment(&api, args).await,
            _ => Err(unknown_action(action)),
        }
    }

    async fn accounts(&self, http: &dyn Http, creds: &Credentials) -> Result<Vec<AccountOption>, ActionError> {
        let user = Api::new(http, creds).get("/user").await?;
        let login = text_of(&user, "/login");
        Ok(vec![AccountOption { id: login.clone(), label: login.clone(), settings: json!({ "login": login }) }])
    }

    fn events(&self) -> Option<&dyn EventSource> {
        Some(&events::GithubEvents)
    }
}

#[cfg(test)]
mod tests;
