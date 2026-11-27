use std::path::{Path, PathBuf};
use std::sync::Arc;

use agent_engine::AgentEngine;
use agent_engine::sessions::FileSessions;

use crate::data_root::DataRoot;
use crate::extensions::Extensions;
use crate::providers::ModelProviders;
use crate::settings::SettingsStore;

type ServersOf = Arc<dyn Fn(Option<&str>) -> Vec<domain::mcp::RemoteMcp> + Send + Sync>;

#[derive(Clone, Default)]
pub struct ConnectedMcp(Option<ServersOf>);

impl ConnectedMcp {
    pub fn new(servers: impl Fn(Option<&str>) -> Vec<domain::mcp::RemoteMcp> + Send + Sync + 'static) -> Self {
        Self(Some(Arc::new(servers)))
    }

    pub fn servers(&self, owner: Option<&str>) -> Vec<domain::mcp::RemoteMcp> {
        self.0.as_ref().map(|servers| servers(owner)).unwrap_or_default()
    }
}

impl std::fmt::Debug for ConnectedMcp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("ConnectedMcp").field(&self.0.is_some()).finish()
    }
}

pub type BrowserAsk = Arc<dyn Fn(&str, &str) -> Option<String> + Send + Sync>;

type BrowserOf = Arc<dyn Fn(&str, Vec<PathBuf>, Option<BrowserAsk>) -> Option<domain::mcp::LocalMcp> + Send + Sync>;

#[derive(Clone, Default)]
pub struct BrowserMcp(Option<BrowserOf>);

impl BrowserMcp {
    pub fn new(
        server: impl Fn(&str, Vec<PathBuf>, Option<BrowserAsk>) -> Option<domain::mcp::LocalMcp> + Send + Sync + 'static,
    ) -> Self {
        Self(Some(Arc::new(server)))
    }

    pub fn for_run(
        &self,
        session: &str,
        roots: Vec<PathBuf>,
        ask: Option<BrowserAsk>,
    ) -> Option<domain::mcp::LocalMcp> {
        self.0.as_ref().and_then(|server| server(session, roots, ask))
    }
}

impl std::fmt::Debug for BrowserMcp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("BrowserMcp").field(&self.0.is_some()).finish()
    }
}

#[derive(Clone)]
pub struct LocalServices {
    pub engine: Arc<dyn AgentEngine>,
    pub root: DataRoot,
    pub settings: Arc<dyn SettingsStore>,
    pub catalog: Arc<dyn crate::SessionCatalog>,
    pub memories: crate::features::memory::Memories,
    pub database: crate::Degraded,
    pub sessions: Arc<FileSessions>,
    pub extensions: Extensions,
    pub providers: ModelProviders,
    pub homes: Arc<dyn crate::Homes>,
    pub mcp_auth: crate::McpAuth,
    pub secrets: Arc<dyn crate::SecretStore>,
    pub connected_mcp: ConnectedMcp,
    pub browser_mcp: BrowserMcp,
    pub(crate) exporter: Option<crate::features::trace::export::Exporter>,
}

fn exporter(sessions: &Arc<FileSessions>) -> Option<crate::features::trace::export::Exporter> {
    use crate::features::trace::export::{Exporter, Otlp, Traced};
    let otlp = Otlp::from(brand::traces::OTLP_ENDPOINT, brand::traces::OTLP_HEADERS)?;
    let store = Arc::clone(sessions);
    let traced: Traced = Arc::new(move |cwd, id| crate::features::trace::traced(&store, cwd, id));
    Exporter::start(otlp, traced, Arc::new(agent_engine::model::http::NetHttp))
}

impl std::fmt::Debug for LocalServices {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LocalServices").field("root", &self.root).finish_non_exhaustive()
    }
}

impl LocalServices {
    pub fn detect() -> Self {
        let root = DataRoot::system();
        let stores = root.stores();
        let engine = crate::engine::build(Arc::clone(&stores.sessions), &root);
        let mut services = Self::of(Arc::new(engine), root, stores);
        services.homes = Arc::new(crate::OwnerHome::system());
        services
    }

    pub fn gate(&self, cwd: &Path) -> agent_engine::scope::Scope {
        agent_engine::scope::Scope::at(cwd, self.home().as_deref())
    }

    pub fn project_settings(
        &self,
        cwd: &Path,
    ) -> Result<agent_engine::prompt::settings::Settings, crate::settings::SettingsError> {
        agent_engine::prompt::settings::load(&self.gate(cwd)).map_err(crate::settings::SettingsError::Unreadable)
    }

    pub fn with_engine(engine: Arc<dyn AgentEngine>) -> Self {
        Self::in_root(engine, DataRoot::in_memory())
    }

    pub fn in_root(engine: Arc<dyn AgentEngine>, root: DataRoot) -> Self {
        let stores = root.stores();
        Self::of(engine, root, stores)
    }

    fn of(engine: Arc<dyn AgentEngine>, root: DataRoot, stores: crate::data_root::Stores) -> Self {
        let http = Arc::new(agent_engine::model::http::NetHttp);
        Self {
            engine,
            root,
            settings: stores.settings,
            catalog: stores.catalog,
            memories: stores.memories,
            database: stores.database,
            exporter: exporter(&stores.sessions),
            sessions: stores.sessions,
            extensions: Extensions,
            providers: ModelProviders::new(stores.providers, Arc::clone(&stores.secrets)),
            homes: Arc::new(crate::OwnerHome::none()),
            mcp_auth: crate::McpAuth::new(Arc::clone(&stores.secrets), http),
            secrets: stores.secrets,
            connected_mcp: ConnectedMcp::default(),
            browser_mcp: BrowserMcp::default(),
        }
    }

    pub fn with_browser_mcp(mut self, browser: BrowserMcp) -> Self {
        self.browser_mcp = browser;
        self
    }

    pub fn with_applications(mut self, applications: crate::features::memory::ApplicationOf) -> Self {
        self.memories = self.memories.with_applications(applications);
        self
    }

    pub fn with_connected_mcp(mut self, connected: ConnectedMcp) -> Self {
        self.connected_mcp = connected;
        self
    }

    pub fn home(&self) -> Option<PathBuf> {
        self.homes.home()
    }

    pub fn kept_in(self, data: &Path) -> Self {
        let root = DataRoot::under(data);
        let stores = root.stores();
        Self {
            root,
            settings: stores.settings,
            catalog: stores.catalog,
            memories: stores.memories,
            database: stores.database,
            exporter: exporter(&stores.sessions),
            sessions: stores.sessions,
            providers: self.providers.with_store(stores.providers).with_secrets(Arc::clone(&stores.secrets)),
            mcp_auth: self.mcp_auth.with_secrets(Arc::clone(&stores.secrets)),
            secrets: stores.secrets,
            ..self
        }
    }
}

#[cfg(test)]
mod tests {
    use agent_engine::fake::FakeEngine;

    use super::*;

    fn engine() -> Arc<dyn AgentEngine> {
        Arc::new(FakeEngine::new(Vec::new(), Vec::new()))
    }

    #[test]
    fn kept_in_keeps_everything_in_the_given_folder() {
        let data = tempfile::tempdir().expect("temp dir");
        let services = LocalServices::with_engine(engine()).kept_in(data.path());
        llm_providers::testing::install();

        services.settings.save(&serde_json::json!({"model": "m"})).expect("save");
        services.catalog.update(&mut |catalog| {
            catalog.started(&domain::SessionId("s".into()), Path::new("/work"), 1);
        });
        services.providers.connect(domain::ModelProvider::new("keyed"), "sk-proj-0123456789abcd");

        let remember = domain::MemoryEdit::Remember {
            kind: domain::MemoryKind::Semantic,
            text: "Uses wgpu".into(),
            tags: Vec::new(),
            touches: Vec::new(),
            scope: Default::default(),
        };
        let by = domain::Provenance { author: domain::Author::User, now: 5, origin: None };
        crate::features::memory::of(&services).edit(Path::new("/work"), &[remember], by).expect("memory");

        for kept in ["app.db", "providers/local/providers.json"] {
            assert!(data.path().join(kept).is_file(), "{kept}");
        }
        assert!(!data.path().join("users/local/sessions.json").exists(), "the catalog is in the database");
        assert!(!data.path().join("memory").exists(), "and so is the memory");
        let reopened = LocalServices::with_engine(engine()).kept_in(data.path());
        assert_eq!(reopened.catalog.read().listed().len(), 1);
        assert_eq!(crate::features::memory::of(&reopened).load(Path::new("/work")).memories.len(), 1);
        assert!(data.path().join("secrets.json").is_file() && data.path().join("secrets.key").is_file());
        assert!(services.root.folders().iter().all(|folder| folder.starts_with(data.path())));
    }

    #[test]
    fn services_built_for_a_test_keep_nothing_on_this_machine() {
        let system = DataRoot::system();
        let before: Vec<bool> = system.folders().iter().map(|folder| folder.exists()).collect();

        let services = LocalServices::with_engine(engine());
        llm_providers::testing::install();
        services.settings.save(&serde_json::json!({"model": "m"})).expect("save");
        services.providers.connect(domain::ModelProvider::new("keyed"), "sk-proj-0123456789abcd");
        services.catalog.update(&mut |catalog| {
            catalog.started(&domain::SessionId("s".into()), Path::new("/work"), 1);
        });

        assert!(services.root.keeps_nothing());
        assert_eq!(services.home(), None, "the machine's home is not read either");
        assert!(services.root.folders().iter().all(|folder| !system.holds(folder) && !folder.exists()));
        assert_eq!(services.settings.load(), Some(serde_json::json!({"model": "m"})), "kept, in memory");
        let after: Vec<bool> = system.folders().iter().map(|folder| folder.exists()).collect();
        assert_eq!(before, after, "nothing was made where the user's data lives");
    }
}
