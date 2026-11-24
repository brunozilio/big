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
pub use api::{Backend, BackendError, Wake};
pub(crate) use catalog::SessionCatalog;
pub use client::{Builder, Client, Error, Headless, Run, RunControl, Session, SignedIn};
pub(crate) use data_root::DataRoot;
pub(crate) use degraded::Degraded;
pub use dirs::project_key;
pub use domain::provider::{ModelProvider, ProviderSummary};
pub use domain::{AgentAsk, AskId, AskKind, Decision, Effort, EngineEvent, ModelOption as Model, PermissionMode, SessionId};
pub use handler::local::LocalBackend;
pub(crate) use homes::{Homes, OwnerHome};
pub use keyed::KeyedEngine;
pub use llm_providers::efforts;
pub(crate) use mcp_auth::McpAuth;
pub use providers::{InMemoryProviderStore, ModelProviders};
pub use secrets::{InMemorySecretStore, SecretStore};

pub mod team {
    pub use agent_engine::team::{Me, Team};
}
pub use services::{BrowserAsk, BrowserMcp, ConnectedMcp, LocalServices};
pub use settings::{SettingsError, SettingsStore};
