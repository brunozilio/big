mod chat;
mod download;
mod engine;
mod output;
mod serve;
mod store;
mod template;

use std::sync::OnceLock;

pub use download::{Request, Transfer, start as download, transfers};
pub use engine::unload;
pub use serve::{endpoint, models};
pub use store::{Stored, folder, remove, stored};

#[derive(Debug, Clone, thiserror::Error, PartialEq, Eq)]
pub enum LocalError {
    #[error("{0}")]
    Bad(String),
    #[error("there is no model called {0} in the models folder")]
    Missing(String),
    #[error("the chat template failed: {0}")]
    Template(String),
    #[error("{0}")]
    Engine(String),
    #[error("{0}")]
    Network(String),
    #[error("{0}")]
    Disk(String),
}

pub(crate) fn runtime() -> Result<&'static tokio::runtime::Runtime, LocalError> {
    static RUNTIME: OnceLock<Result<tokio::runtime::Runtime, String>> = OnceLock::new();
    RUNTIME
        .get_or_init(|| {
            tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                .thread_name("local-models")
                .enable_all()
                .build()
                .map_err(|error| error.to_string())
        })
        .as_ref()
        .map_err(|error| LocalError::Engine(error.clone()))
}
