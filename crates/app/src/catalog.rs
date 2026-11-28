pub(crate) mod former;
pub(crate) mod legacy;
mod sqlite;
#[cfg(test)]
mod tests;

use std::path::PathBuf;
use std::sync::Mutex;

use domain::SessionId;
use domain::catalog::Catalog;
use platform::sync::Locked;

pub use sqlite::SqliteSessionCatalog;

pub trait SessionCatalog: Send + Sync {
    fn read(&self) -> Catalog;
    fn update(&self, change: &mut (dyn FnMut(&mut Catalog) + Send));

    fn is_empty(&self) -> bool {
        self.read().listed().is_empty()
    }
}

#[derive(Debug, Default)]
pub struct InMemorySessionCatalog {
    kept: Mutex<Catalog>,
}

impl SessionCatalog for InMemorySessionCatalog {
    fn read(&self) -> Catalog {
        self.kept.locked().clone()
    }

    fn update(&self, change: &mut (dyn FnMut(&mut Catalog) + Send)) {
        change(&mut self.kept.locked());
    }
}

pub(crate) struct Listing {
    catalog: std::sync::Arc<dyn SessionCatalog>,
    cwd: PathBuf,
    session: Mutex<Option<SessionId>>,
}

impl Listing {
    pub(crate) fn of(services: &crate::LocalServices, request: &domain::PromptRequest) -> Option<Self> {
        let chore = request.session.command.is_some_and(domain::SessionCommand::is_chore);
        (!chore).then(|| Self {
            catalog: std::sync::Arc::clone(&services.catalog),
            cwd: request.session.cwd.clone(),
            session: Mutex::new(None),
        })
    }

    pub(crate) fn observe(&self, event: &domain::EngineEvent) {
        use domain::EngineEvent as Event;
        use domain::conversation::SessionStatus as Status;
        if let Event::SessionStarted(session) = event {
            *self.session.locked() = Some(session.clone());
            self.catalog.update(&mut |catalog| catalog.started(session, &self.cwd, now()));
            return;
        }
        let status = match event {
            Event::Asked(_) => Status::NeedsYou,
            Event::AskWithdrawn(_) | Event::Answered { .. } => Status::Running,
            Event::Completed { .. } => Status::Done,
            Event::Failed(_) => Status::Failed,
            Event::Cancelled => Status::Cancelled,
            _ => return,
        };
        if let Some(session) = self.session.locked().clone() {
            self.catalog.update(&mut |catalog| catalog.status(&session, status, now()));
        }
    }
}

fn now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |since| since.as_secs())
}
