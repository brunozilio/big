use async_trait::async_trait;
use domain::Language;
use domain::connection::{Capability, ProviderKey};
use jiff::Timestamp;
use serde_json::{Value, json};

use crate::contracts::{self, mcp as actions};
use crate::descriptor::{AlternateAuth, AuthSpec, Presentation, ProviderDescriptor, TokenField, TokenForm};
use crate::error::ActionError;
use crate::http::Http;
use crate::mcp::McpClient;
use crate::oauth::{Authorization, Grant, Loopback, LoopbackSignIn};
use crate::provider::{AccountOption, Credentials, Provider, unknown_action};
use crate::providers::mcp_server::{McpServers, ServerSignIn};

pub const KEY: &str = "datadog";
pub const SITE: &str = "site";
pub const ACCESS_TOKEN: &str = "access_token";
const LOGO: &str = include_str!("logo.svg");

pub const SITES: [(&str, &str); 7] = [
    ("datadoghq.com", "US1"),
    ("us3.datadoghq.com", "US3"),
    ("us5.datadoghq.com", "US5"),
    ("datadoghq.eu", "EU1"),
    ("ap1.datadoghq.com", "AP1"),
    ("ap2.datadoghq.com", "AP2"),
    ("uk1.datadoghq.com", "UK1"),
];

#[derive(Debug, Clone, Copy, Default)]
pub struct Datadog;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DatadogSignIn {
    pub site: String,
}

pub fn site_of(entered: &str) -> Option<&'static str> {
    let bare = entered.trim().trim_start_matches("https://").trim_start_matches("http://");
    let bare = bare.split('/').next().unwrap_or_default().to_ascii_lowercase();
    let bare = bare.strip_prefix("app.").or_else(|| bare.strip_prefix("mcp.")).unwrap_or(bare.as_str());
    let bare = if bare.is_empty() { brand::connectors::DATADOG_SITE } else { bare };
    SITES.iter().map(|(site, _)| *site).find(|site| *site == bare)
}

pub fn server_of(site: &str) -> String {
    brand::connectors::DATADOG_MCP_URL.replace("{site}", site)
}

pub fn label_of(site: &str) -> String {
    let region = SITES.iter().find(|(known, _)| *known == site).map(|(_, region)| *region);
    match region {
        Some(region) if site != brand::connectors::DATADOG_SITE => format!("Datadog {region}"),
        _ => "Datadog".to_owned(),
    }
}

fn unknown_site(language: Language) -> String {
    let sites: Vec<&str> = SITES.iter().map(|(site, _)| *site).collect();
    let sites = sites.join(", ");
    language.pick(
        format!("Use one of the Datadog sites: {sites}."),
        format!("Usa uno de los sitios de Datadog: {sites}."),
        format!("Use um dos sites do Datadog: {sites}."),
    )
}

fn site_field(language: Language) -> TokenField {
    let label = language.pick("Datadog site", "Sitio de Datadog", "Site do Datadog");
    TokenField::setting(SITE, label, "").filled(brand::connectors::DATADOG_SITE)
}

pub fn access_token(language: Language) -> AlternateAuth {
    let label = language.pick("Datadog access token", "Token de acceso de Datadog", "Token de acesso do Datadog");
    let help = language.pick(
        "In Datadog, create a personal access token (or a service account token) with the mcp_read scope, and mcp_write if agents may change things.",
        "En Datadog, crea un personal access token (o un token de service account) con el scope mcp_read, y mcp_write si los agentes pueden cambiar cosas.",
        "No Datadog, crie um personal access token (ou um token de service account) com o scope mcp_read, e mcp_write se os agentes puderem alterar algo.",
    );
    let token = TokenField::new("token", label, help, true);
    let way = language.pick("Use an access token", "Usar un token de acceso", "Usar token de acesso");
    let site = language.pick("Datadog", "de Datadog", "do Datadog");
    AlternateAuth::token(language, ACCESS_TOKEN, way, site, vec![site_field(language), token], TokenForm::Bearer)
}

impl DatadogSignIn {
    fn server(&self) -> ServerSignIn {
        let site = site_of(&self.site).unwrap_or(brand::connectors::DATADOG_SITE);
        ServerSignIn { server: server_of(site) }
    }
}

#[async_trait]
impl LoopbackSignIn for DatadogSignIn {
    fn name(&self) -> &str {
        KEY
    }

    fn fields(&self, language: Language) -> Vec<TokenField> {
        vec![site_field(language)]
    }

    fn for_fields(&self, fields: &Value, language: Language) -> Result<Option<Loopback>, String> {
        let entered = fields[SITE].as_str().unwrap_or_default();
        let site = site_of(entered).ok_or_else(|| unknown_site(language))?;
        Ok(Some(Loopback::new(DatadogSignIn { site: site.to_owned() })))
    }

    async fn begin(&self, http: &dyn Http, port: u16) -> Result<Authorization, ActionError> {
        self.server().begin(http, port).await
    }

    async fn exchange(
        &self,
        http: &dyn Http,
        authorization: &Authorization,
        code: &str,
        now: Timestamp,
    ) -> Result<Grant, ActionError> {
        self.server().exchange(http, authorization, code, now).await
    }

    async fn refresh(&self, http: &dyn Http, grant: &Grant, now: Timestamp) -> Result<Grant, ActionError> {
        self.server().refresh(http, grant, now).await
    }
}

fn site_in(creds: &Credentials) -> Result<&'static str, ActionError> {
    let entered = creds.setting(SITE).unwrap_or_default();
    site_of(entered).ok_or_else(|| ActionError::permanent(unknown_site(Language::English)))
}

#[async_trait]
impl Provider for Datadog {
    fn descriptor(&self) -> ProviderDescriptor {
        ProviderDescriptor {
            key: ProviderKey::from(KEY),
            name: "Datadog".to_owned(),
            logo: LOGO.to_owned(),
            capabilities: vec![Capability::from(contracts::MCP)],
            auth: AuthSpec::Loopback(Loopback::new(DatadogSignIn::default())),
            actions: contracts::mcp(),
        }
    }

    fn presentation(&self, language: Language) -> Presentation {
        Presentation {
            description: language.pick("Your Datadog account", "Tu cuenta de Datadog", "Sua conta Datadog").to_owned(),
            group: "mcp".to_owned(),
            button: language.pick("Continue on Datadog", "Continuar en Datadog", "Continuar no Datadog").to_owned(),
            site: language.pick("Datadog", "de Datadog", "do Datadog").to_owned(),
            choose_title: language.pick("Datadog site", "Sitio de Datadog", "Site do Datadog").to_owned(),
            hidden: false,
        }
    }

    fn alternatives(&self, language: Language) -> Vec<AlternateAuth> {
        vec![access_token(language)]
    }

    async fn call(
        &self,
        http: &dyn Http,
        creds: &Credentials,
        action: &str,
        args: &Value,
        idempotency_key: &str,
    ) -> Result<Value, ActionError> {
        match action {
            actions::TOOLS => McpServers.call(http, creds, action, args, idempotency_key).await,
            _ => Err(unknown_action(action)),
        }
    }

    async fn accounts(&self, http: &dyn Http, creds: &Credentials) -> Result<Vec<AccountOption>, ActionError> {
        let site = site_in(creds)?;
        let url = server_of(site);
        let found = McpClient::connect(http, &url, &creds.access_token).await?.tools().await?;
        let settings = json!({
            contracts::mcp::URL: url,
            SITE: site,
            "shown": { "tools": found.len(), "host": site },
        });
        Ok(vec![AccountOption { id: site.to_owned(), label: label_of(site), settings }])
    }
}

#[cfg(test)]
mod tests;
