use std::sync::Arc;

#[derive(Debug, Clone, Default)]
pub struct Services {
    pub network: Option<Network>,
    pub lsp: Option<crate::lsp::Lsp>,
    pub files: Option<Arc<crate::checkpoints::FileHistory>>,
    pub deferred: Option<Arc<crate::tools::tool_search::Deferred>>,
    pub search: Option<WebSearcher>,
    pub launch: super::Launch,
    pub mcp: Vec<Arc<crate::mcp::McpServer>>,
    pub output_schema: Option<serde_json::Value>,
    pub artifacts: Option<std::path::PathBuf>,
    pub pointers: Option<CodePointers>,
    pub history: Option<crate::history::HostHistory>,
}

pub trait Pointers: Send + Sync {
    fn pointers(&self, project: &std::path::Path, prompt: &str) -> Option<String>;
}

#[derive(Clone)]
pub struct CodePointers(pub Arc<dyn Pointers>);

impl CodePointers {
    pub fn for_task(&self, project: &std::path::Path, task: &str) -> Option<String> {
        self.0.pointers(project, task).map(|map| map.trim().to_owned()).filter(|map| !map.is_empty())
    }
}

impl std::fmt::Debug for CodePointers {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("CodePointers")
    }
}

#[derive(Clone)]
pub struct WebSearcher(pub Arc<dyn crate::model::Searcher>);

impl std::fmt::Debug for WebSearcher {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("WebSearcher")
    }
}

#[derive(Clone)]
pub struct Network(pub Arc<dyn crate::model::http::Http>);

impl std::fmt::Debug for Network {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Network")
    }
}
