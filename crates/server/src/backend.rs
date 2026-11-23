use std::path::PathBuf;
use std::sync::Arc;

use connectors::Connectors;
use orchestrator::Orchestrator;
use store::Store;

#[derive(Clone)]
pub struct Backend {
    pub orchestrator: Orchestrator,
    pub store: Arc<Store>,
    pub connectors: Arc<Connectors>,
    pub client: app::Client,
    pub engine: Option<Arc<app::KeyedEngine>>,
    pub data: PathBuf,
}

impl std::fmt::Debug for Backend {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_struct("Backend").field("data", &self.data).finish_non_exhaustive()
    }
}

impl Backend {
    pub fn attachments(&self) -> PathBuf {
        self.data.join("attachments")
    }
}
