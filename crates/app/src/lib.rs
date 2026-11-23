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
mod git;
mod handler;
mod homes;
mod mcp_admin;
mod mcp_auth;
mod permissions;
mod providers;
mod secrets;
mod services;
mod settings;

pub use api::features::plugins::{Plugin, PluginsReply, PluginsRequest};
pub use api::features::shell::BackgroundCommand;
pub use api::{Backend, BackendError, Wake};
pub use catalog::{InMemorySessionCatalog, SessionCatalog, SqliteSessionCatalog};
pub use client::{Builder, Client, Error, Headless, ProjectMemory, Run, RunControl, Session, SignedIn, Spending};
pub use data_root::DataRoot;
pub use db::Db;
pub use degraded::Degraded;
pub use dirs::project_key;
pub use domain::mcp_config::{McpEntry, McpListing, McpOrigin, McpServerInfo, McpState, McpUnreadable};
pub use domain::memory::seed::Candidate as SeedCandidate;
pub use domain::memory::{Forgotten, Memory, MemoryEdit, MemoryId, MemoryKind, Revision};
pub use domain::provider::{ModelProvider, PortHolder, ProviderSummary};
pub use domain::schedule::{Scheduled, When};
pub use domain::{
    AgentAsk, AskId, AskKind, Choice, ChoreAnswer, Criteria, CriteriaSource, CustomAgent, Decision, Effort,
    EngineEvent, HeadsUp, Image, MAX_IMAGE_BYTES, ModelOption as Model, PermissionMode, Question, Rewound, SessionId,
    ToolCall, Verdict,
};
pub use extensions::Extensions;
pub use handler::CommandHandler;
pub use handler::local::LocalBackend;
pub use handler::outbox::{Outbox, Replies};
pub use homes::{Homes, OwnerHome};
pub use mcp_auth::{McpAuth, McpAuthError};
pub use providers::{InMemoryProviderStore, ModelCatalog, ModelProviders, ProviderStore};
pub use secrets::{InMemorySecretStore, SecretStore, Secrets};

pub mod team {
    pub use agent_engine::team::{Me, Team};
}
pub use services::LocalServices;
pub use settings::{SettingsChange, SettingsError, SettingsStore};
