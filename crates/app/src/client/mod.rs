mod account;
mod catalog;
mod chores;
mod dispatch;
mod limits;
mod run;
mod session;
#[cfg(test)]
mod testing;

use std::path::PathBuf;
use std::sync::Arc;

use crate::api::Wake;
use crate::{LocalBackend, LocalServices};
use domain::ModelOption;

pub use self::account::SignedIn;
pub use self::run::{Run, RunControl};
pub use self::session::{Headless, Session};
use dispatch::Dispatcher;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Backend(String),
    #[error("the backend did not answer in time")]
    Timeout,
    #[error("the run failed: {0}")]
    Run(String),
    #[error("there is no conversation to ask about yet")]
    NothingToAsk,
}

impl From<domain::failure::RunFailure> for Error {
    fn from(failure: domain::failure::RunFailure) -> Self {
        Self::Run(failure.message)
    }
}

#[derive(Debug, Clone)]
pub struct Client {
    dispatcher: Arc<Dispatcher>,
    services: LocalServices,
}

impl Client {
    pub fn builder() -> Builder {
        Builder::default()
    }

    pub fn sibling(&self) -> Result<Self, Error> {
        Self::local(self.services.clone())
    }

    fn local(services: LocalServices) -> Result<Self, Error> {
        let (wake_tx, woken) = crossbeam_channel::bounded(1);
        let wake: Wake = Arc::new(move || {
            let _ = wake_tx.try_send(());
        });
        let backend = LocalBackend::new(services.clone(), wake).map_err(|error| Error::Backend(error.to_string()))?;
        Ok(Self { dispatcher: Arc::new(Dispatcher::new(Box::new(backend), woken)), services })
    }

    pub fn session(&self, cwd: impl Into<PathBuf>, model: ModelOption) -> Session {
        let cwd = cwd.into();
        let cwd = std::path::absolute(&cwd).unwrap_or(cwd);
        Session::new(Arc::clone(&self.dispatcher), cwd, model)
    }
}

#[derive(Default)]
pub struct Builder {
    services: Option<LocalServices>,
}

impl std::fmt::Debug for Builder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Builder").finish_non_exhaustive()
    }
}

impl Builder {
    pub fn services(mut self, services: LocalServices) -> Self {
        self.services = Some(services);
        self
    }

    pub fn build(self) -> Result<Client, Error> {
        Client::local(self.services.unwrap_or_else(LocalServices::detect))
    }
}
