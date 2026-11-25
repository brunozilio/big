use async_trait::async_trait;
use domain::Language;
use domain::connection::{Capability, ProviderKey};
use domain::mcp_oauth;
use jiff::Timestamp;
use serde_json::{Value, json};

use crate::contracts::{self, mcp as actions};
use crate::descriptor::{AuthSpec, Presentation, ProviderDescriptor, TokenField};
use crate::error::{ActionError, ErrorClass};
use crate::http::Http;
use crate::mcp::McpClient;
use crate::oauth::mcp::McpSignIn;
use crate::oauth::{Authorization, Grant, Loopback, LoopbackSignIn};
use crate::provider::{AccountOption, Credentials, Provider, unknown_action};

pub const KEY: &str = "mcp";
pub const LABEL: &str = "label";
const LOGO: &str = include_str!("logo.svg");
const RESOURCE: &str = "resource";

#[derive(Debug, Clone, Copy, Default)]
pub struct McpServers;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ServerSignIn {
    pub server: String,
}

pub fn entered_url(fields: &Value, language: Language) -> Result<String, String> {
    let url = fields[contracts::mcp::URL].as_str().map(str::trim).unwrap_or_default();
    if url.is_empty() {
        let told = language.pick(
            "Enter the MCP server address.",
            "Indica la dirección del servidor MCP.",
            "Informe o endereço do servidor MCP.",
        );
        return Err(told.to_owned());
    }
    if !mcp_oauth::endpoint_ok(url) {
        let told = language.pick(
            "Use an https address, or http only on localhost.",
            "Usa una dirección https, o http solo en localhost.",
            "Use um endereço https, ou http só em localhost.",
        );
        return Err(told.to_owned());
    }
    Ok(url.to_owned())
}

pub fn host_of(url: &str) -> String {
    domain::site::site(url).unwrap_or_else(|| url.to_owned())
}

impl ServerSignIn {
    fn flow(&self, server: &str) -> McpSignIn {
        McpSignIn {
            server: server.to_owned(),
            redirect: brand::connectors::OAUTH_REDIRECT.to_owned(),
            client_name: brand::NAME.to_owned(),
            scopes: Vec::new(),
        }
    }

    fn server(&self) -> Result<&str, ActionError> {
        if self.server.is_empty() { Err(ActionError::permanent("no MCP server was given")) } else { Ok(&self.server) }
    }
}

#[async_trait]
impl LoopbackSignIn for ServerSignIn {
    fn name(&self) -> &str {
        if self.server.is_empty() { KEY } else { &self.server }
    }

    fn fields(&self, language: Language) -> Vec<TokenField> {
        let address = language.pick("MCP server address", "Dirección del servidor MCP", "Endereço do servidor MCP");
        let example = language.pick(
            "For example, https://mcp.example.com/mcp.",
            "Por ejemplo, https://mcp.ejemplo.com/mcp.",
            "Por exemplo, https://mcp.exemplo.com/mcp.",
        );
        let name = language.pick("Name", "Nombre", "Nome");
        let help = language.pick(
            "Agents see the tools as mcp__name__tool.",
            "Los agentes ven las herramientas como mcp__nombre__herramienta.",
            "Os agentes veem as ferramentas como mcp__nome__ferramenta.",
        );
        vec![TokenField::new(contracts::mcp::URL, address, example, false), TokenField::new(LABEL, name, help, false)]
    }

    fn for_fields(&self, fields: &Value, language: Language) -> Result<Option<Loopback>, String> {
        let server = entered_url(fields, language)?;
        Ok(Some(Loopback::new(ServerSignIn { server })))
    }

    async fn without_sign_in(&self, http: &dyn Http) -> Result<Option<Grant>, ActionError> {
        match McpClient::connect(http, self.server()?, "").await {
            Ok(_) => Ok(Some(Grant::bearer(""))),
            Err(error) if error.class == ErrorClass::Auth => Ok(None),
            Err(error) => Err(error),
        }
    }

    async fn begin(&self, http: &dyn Http, port: u16) -> Result<Authorization, ActionError> {
        self.flow(self.server()?).begin(http, port).await
    }

    async fn exchange(
        &self,
        http: &dyn Http,
        authorization: &Authorization,
        code: &str,
        now: Timestamp,
    ) -> Result<Grant, ActionError> {
        let server = self.server()?;
        let mut grant = self.flow(server).exchange(http, authorization, code, now).await?;
        grant.extra[RESOURCE] = json!(server);
        Ok(grant)
    }

    async fn refresh(&self, http: &dyn Http, grant: &Grant, now: Timestamp) -> Result<Grant, ActionError> {
        let server = grant.extra[RESOURCE].as_str().filter(|server| !server.is_empty()).unwrap_or(&self.server);
        if server.is_empty() {
            return Err(ActionError::auth("the sign-in lost its MCP server"));
        }
        self.flow(server).refresh(http, grant, now).await
    }
}

fn url_of(creds: &Credentials) -> Result<&str, ActionError> {
    creds.setting(contracts::mcp::URL).ok_or_else(|| ActionError::missing(contracts::mcp::URL))
}

async fn tools(http: &dyn Http, creds: &Credentials) -> Result<Vec<String>, ActionError> {
    McpClient::connect(http, url_of(creds)?, &creds.access_token).await?.tools().await
}

#[async_trait]
impl Provider for McpServers {
    fn descriptor(&self) -> ProviderDescriptor {
        ProviderDescriptor {
            key: ProviderKey::from(KEY),
            name: "MCP server".to_owned(),
            logo: LOGO.to_owned(),
            capabilities: vec![Capability::from(contracts::MCP)],
            auth: AuthSpec::Loopback(Loopback::new(ServerSignIn::default())),
            actions: contracts::mcp(),
        }
    }

    fn presentation(&self, language: Language) -> Presentation {
        Presentation {
            description: language
                .pick(
                    "Tools from a remote MCP server for the agents",
                    "Herramientas de un servidor MCP remoto para los agentes",
                    "Ferramentas de um servidor MCP remoto para os agentes",
                )
                .to_owned(),
            group: "mcp".to_owned(),
            button: crate::texts::connect(language).to_owned(),
            site: language.pick("the MCP server", "del servidor MCP", "do servidor MCP").to_owned(),
            choose_title: language.pick("MCP server", "Servidor MCP", "Servidor MCP").to_owned(),
            hidden: true,
        }
    }

    async fn call(
        &self,
        http: &dyn Http,
        creds: &Credentials,
        action: &str,
        _args: &Value,
        _idempotency_key: &str,
    ) -> Result<Value, ActionError> {
        match action {
            actions::TOOLS => {
                let names = tools(http, creds).await?;
                Ok(Value::Array(names.into_iter().map(|name| json!({ "name": name })).collect()))
            }
            _ => Err(unknown_action(action)),
        }
    }

    async fn accounts(&self, http: &dyn Http, creds: &Credentials) -> Result<Vec<AccountOption>, ActionError> {
        let url = url_of(creds)?.to_owned();
        let found = tools(http, creds).await?;
        let host = host_of(&url);
        let label = creds.setting(LABEL).map_or_else(|| host.clone(), str::to_owned);
        let settings = json!({ contracts::mcp::URL: url, "shown": { "tools": found.len(), "host": host } });
        Ok(vec![AccountOption { id: url, label, settings }])
    }
}

#[cfg(test)]
mod tests;
