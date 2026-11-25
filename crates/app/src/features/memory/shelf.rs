use std::path::{Path, PathBuf};
use std::sync::Arc;

use domain::MemoryScope;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApplicationRef {
    pub id: String,
    pub name: String,
}

pub type ApplicationOf = Arc<dyn Fn(&Path) -> Option<ApplicationRef> + Send + Sync>;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum MemoryShelf {
    Repository(PathBuf),
    Application(String),
    Global,
}

impl MemoryShelf {
    pub fn scope(&self) -> MemoryScope {
        match self {
            MemoryShelf::Repository(_) => MemoryScope::Repository,
            MemoryShelf::Application(_) => MemoryScope::Application,
            MemoryShelf::Global => MemoryScope::Global,
        }
    }

    pub(crate) fn key(&self) -> String {
        match self {
            MemoryShelf::Repository(project) => crate::dirs::memory_key(project),
            MemoryShelf::Application(id) => format!("application:{id}"),
            MemoryShelf::Global => "global".to_owned(),
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct Shelves {
    pub repository: PathBuf,
    pub application: Option<ApplicationRef>,
}

impl Shelves {
    pub fn of(&self, scope: MemoryScope) -> Option<MemoryShelf> {
        match scope {
            MemoryScope::Repository => Some(MemoryShelf::Repository(self.repository.clone())),
            MemoryScope::Application => self.application.as_ref().map(|it| MemoryShelf::Application(it.id.clone())),
            MemoryScope::Global => Some(MemoryShelf::Global),
        }
    }

    pub fn all(&self) -> Vec<MemoryShelf> {
        MemoryScope::ALL.into_iter().filter_map(|scope| self.of(scope)).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_three_shelves_never_share_a_key() {
        let shelves = [
            MemoryShelf::Repository("/work/global".into()),
            MemoryShelf::Application("global".into()),
            MemoryShelf::Global,
        ];
        let keys: std::collections::HashSet<String> = shelves.iter().map(MemoryShelf::key).collect();
        assert_eq!(keys.len(), 3);
    }

    #[test]
    fn a_repository_outside_every_application_reads_only_its_own_and_the_global_memory() {
        let alone = Shelves { repository: "/work/x".into(), application: None };
        assert_eq!(alone.all(), vec![MemoryShelf::Repository("/work/x".into()), MemoryShelf::Global]);
        let grouped = Shelves { application: Some(ApplicationRef { id: "a".into(), name: "Rocket".into() }), ..alone };
        assert_eq!(grouped.all().len(), 3);
    }
}
