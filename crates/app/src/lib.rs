pub mod api;
mod catalog;
mod client;
mod data_root;
mod db;
mod degraded;
mod dirs;
mod engine;
mod extensions;
mod features;
mod handler;
mod homes;
mod keyed;
mod mcp_auth;
mod prepare;
mod providers;
mod secrets;
mod services;
mod sessions;
mod settings;

pub use agent_engine::meter::estimate as estimated_cost;
pub use agent_engine::labels;
pub use agent_engine::tools::env_secrets;
pub use agent_engine::policy_gate;
pub use agent_engine::spend_sink::install as install_spend;
pub use agent_engine::prompt::output_style::{OutputStyle, all as output_styles};
pub use api::{Backend, BackendError, BackgroundJob, Wake};
pub(crate) use catalog::SessionCatalog;
pub use client::{Builder, Client, Error, Headless, Run, RunControl, Session, SignedIn};
pub(crate) use data_root::DataRoot;
pub(crate) use degraded::Degraded;
pub use dirs::project_key;
pub use domain::provider::{ModelProvider, ProviderSummary};
pub use domain::{
    AgentAsk, AskId, AskKind, Decision, Effort, EngineEvent, ModelOption as Model, PermissionMode, SessionId,
};
pub use features::memory::{ApplicationOf, ApplicationRef, MemoryShelf};
pub use handler::local::LocalBackend;
pub(crate) use homes::{Homes, OwnerHome};
pub use keyed::KeyedEngine;
pub use llm_providers::{efforts, logo, maker};
pub(crate) use mcp_auth::McpAuth;
pub use providers::{InMemoryProviderStore, ModelProviders};
pub use secrets::{InMemorySecretStore, SecretStore};

pub mod team {
    pub use agent_engine::team::{Me, Team};
}
pub use services::{BrowserAsk, BrowserMcp, ConnectedMcp, LocalServices};
pub use settings::{SettingsError, SettingsStore};

pub fn provider_fields(provider: ModelProvider, language: domain::Language) -> serde_json::Value {
    llm_providers::providers::find(provider).map_or(serde_json::Value::Null, |found| found.fields(language))
}

pub fn release_local_models() {
    llm_providers::release_local_models();
}
