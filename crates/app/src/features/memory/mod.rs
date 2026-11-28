mod gate;
pub(crate) mod lesson;
pub mod mcp;
pub mod reading;
mod recalled;
pub mod run;
mod score;
mod shelf;
pub(crate) mod store;
mod team;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::api::memory::{MemoryApplied, MemoryListing};
use domain::memory::scorecard::RunOutcome;
use domain::{
    Author, Forgotten, Memory, MemoryBook, MemoryBrief, MemoryEdit, MemoryId, MemoryScope, PromptRequest, Provenance,
    RunId,
};
use domain::label::Reads;
use domain::redact::Redactor;
use platform::sync::Locked;
use store::{InMemoryStore, MemoryStore, Revised, SqliteMemoryStore};

use crate::services::LocalServices;
use shelf::Shelves;
pub use shelf::{ApplicationOf, ApplicationRef, MemoryShelf};

type Books = HashMap<String, Revised>;

#[derive(Clone)]
pub struct Memories {
    store: Arc<dyn MemoryStore>,
    books: Arc<Mutex<Books>>,
    homes: Arc<dyn crate::Homes>,
    applications: Option<ApplicationOf>,
    redactor: Arc<Redactor>,
}

impl std::fmt::Debug for Memories {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Memories").finish_non_exhaustive()
    }
}

pub fn of(services: &LocalServices) -> Memories {
    services.memories.clone().with_homes(Arc::clone(&services.homes))
}

fn shown(scope: MemoryScope, memory: &Memory) -> Memory {
    Memory { id: scope.shown(memory.id), ..memory.clone() }
}

impl Memories {
    pub fn new(store: Arc<dyn MemoryStore>) -> Self {
        Self {
            store,
            books: Arc::default(),
            homes: Arc::new(crate::OwnerHome::none()),
            applications: None,
            redactor: agent_engine::tools::redactor(),
        }
    }

    fn gate(&self, project: &Path) -> agent_engine::scope::Scope {
        agent_engine::scope::Scope::at(project, self.home().as_deref())
    }

    pub(crate) fn kept_in(db: crate::db::Db) -> Self {
        Self::new(Arc::new(SqliteMemoryStore::new(db)))
    }

    pub(crate) fn only_in_the_process() -> Self {
        Self::new(Arc::new(InMemoryStore::default()))
    }

    pub fn with_redactor(mut self, redactor: Arc<Redactor>) -> Self {
        self.redactor = redactor;
        self
    }

    pub fn redactor(&self) -> &Arc<Redactor> {
        &self.redactor
    }

    pub fn with_homes(mut self, homes: Arc<dyn crate::Homes>) -> Self {
        self.homes = homes;
        self
    }

    pub fn with_applications(mut self, applications: ApplicationOf) -> Self {
        self.applications = Some(applications);
        self
    }

    fn home(&self) -> Option<PathBuf> {
        self.homes.home()
    }

    fn shelves(&self, project: &Path) -> Shelves {
        let checkout = crate::dirs::main_checkout(project);
        let application = self.applications.as_ref().and_then(|of| of(&checkout));
        Shelves { repository: project.to_path_buf(), application }
    }

    fn kept<'a>(&self, books: &'a mut MutexGuard<Books>, shelf: &MemoryShelf) -> &'a mut Revised {
        let key = shelf.key();
        let stored = self.store.revision(&key);
        let kept = books.entry(key.clone()).or_insert_with(|| self.store.read(&key));
        if kept.revision != stored {
            *kept = self.store.read(&key);
        }
        kept
    }

    fn lock(&self) -> MutexGuard<'_, Books> {
        self.books.locked()
    }

    fn update(&self, shelf: &MemoryShelf, change: &mut (dyn FnMut(&mut MemoryBook) + Send)) -> std::io::Result<()> {
        let mut books = self.lock();
        let changed = self.store.update(&shelf.key(), change)?;
        books.insert(shelf.key(), changed);
        Ok(())
    }

    fn known(&self, shelves: &Shelves) -> (Vec<Memory>, Vec<Forgotten>) {
        let mut books = self.lock();
        let (mut memories, mut forgotten) = (Vec::new(), Vec::new());
        for shelf in shelves.all() {
            let scope = shelf.scope();
            let book = &self.kept(&mut books, &shelf).book;
            memories.extend(book.memories.iter().map(|memory| shown(scope, memory)));
            forgotten.extend(book.forgotten.iter().map(|f| Forgotten { memory: shown(scope, &f.memory), at: f.at }));
        }
        (memories, forgotten)
    }

    pub fn load(&self, project: &Path) -> MemoryListing {
        let (memories, forgotten) = self.known(&self.shelves(project));
        MemoryListing { path: project.to_path_buf(), memories, forgotten }
    }

    pub fn shelf(&self, shelf: &MemoryShelf) -> MemoryListing {
        let mut books = self.lock();
        let book = &self.kept(&mut books, shelf).book;
        let path = match shelf {
            MemoryShelf::Repository(project) => project.clone(),
            _ => PathBuf::new(),
        };
        MemoryListing { path, memories: book.memories.clone(), forgotten: book.forgotten.clone() }
    }

    pub fn shared(&self, project: &Path) -> Vec<Memory> {
        team::load_in(project, &self.redactor)
    }

    pub fn application_of(&self, project: &Path) -> Option<ApplicationRef> {
        self.shelves(project).application
    }

    pub fn edit(&self, project: &Path, edits: &[MemoryEdit], by: Provenance) -> Result<MemoryListing, String> {
        let shelves = self.shelves(project);
        self.set_undo_point(project, None);
        let mut grouped: Vec<(MemoryShelf, Vec<MemoryEdit>)> = Vec::new();
        for edit in edits {
            let (scope, edit) = edit.clone().routed();
            let remembers = matches!(edit, MemoryEdit::Remember { .. });
            let fallback = || remembers.then(|| MemoryShelf::Repository(project.to_path_buf()));
            let Some(shelf) = shelves.of(scope).or_else(fallback) else { continue };
            match grouped.iter_mut().find(|(kept, _)| *kept == shelf) {
                Some((_, kept)) => kept.push(edit),
                None => grouped.push((shelf, vec![edit])),
            }
        }
        for (shelf, edits) in &grouped {
            self.apply(shelf, edits, &by)?;
        }
        Ok(self.load(project))
    }

    pub fn edit_shelf(
        &self,
        shelf: &MemoryShelf,
        edits: &[MemoryEdit],
        by: Provenance,
    ) -> Result<MemoryListing, String> {
        let purges = edits.iter().any(|edit| matches!(edit, MemoryEdit::Purge { .. } | MemoryEdit::PurgeSource { .. }));
        let before = purges.then(|| everything(&self.shelf(shelf)));
        self.apply(shelf, edits, &by)?;
        let listing = self.shelf(shelf);
        if let Some(before) = before {
            reading::purged(&before, &everything(&listing));
        }
        Ok(listing)
    }

    fn apply(&self, shelf: &MemoryShelf, edits: &[MemoryEdit], by: &Provenance) -> Result<(), String> {
        let edits: Vec<MemoryEdit> = edits.iter().map(|edit| edit.redacted(&self.redactor)).collect();
        self.update(shelf, &mut |book| drop(book.apply(&edits, by))).map_err(|error| {
            tracing::warn!(%error, "could not save memory");
            format!("could not save memory: {error}")
        })
    }

    pub fn move_memory(&self, from: &MemoryShelf, id: MemoryId, to: &MemoryShelf) -> Result<(), String> {
        if from == to {
            return Ok(());
        }
        let found = self.shelf(from).memories.into_iter().find(|memory| memory.id == id);
        let Some(memory) = found else { return Err(format!("there is no memory {}", id.0)) };
        let failed = |error: std::io::Error| format!("could not move the memory: {error}");
        self.update(to, &mut |book| {
            book.adopt(memory.clone());
        })
        .map_err(failed)?;
        self.update(from, &mut |book| drop(book.take(id))).map_err(failed)
    }

    fn set_undo_point(&self, project: &Path, book: Option<MemoryBook>) {
        let key = MemoryShelf::Repository(project.to_path_buf()).key();
        if let Err(error) = self.store.set_undo_point(&key, book.as_ref()) {
            tracing::warn!(%error, "could not keep the point the memory can be undone to");
        }
    }

    fn mark_used(&self, shelves: &Shelves, given: &[Memory]) {
        let at = now();
        for shelf in shelves.all() {
            let ids: Vec<MemoryId> = given
                .iter()
                .map(|memory| MemoryScope::of(memory.id))
                .filter(|(scope, _)| *scope == shelf.scope())
                .map(|(_, id)| id)
                .collect();
            if ids.is_empty() {
                continue;
            }
            if let Err(error) = self.update(&shelf, &mut |stored| stored.mark_used(&ids, at)) {
                tracing::warn!(%error, "could not save memory use");
            }
        }
    }

    pub fn finish(
        &self,
        request: &PromptRequest,
        outcome: &RunOutcome,
        reads: &Reads,
    ) -> Option<Result<MemoryApplied, String>> {
        let brief = request.host.memory.as_ref()?;
        self.keep_score(request, brief.held_out, outcome);
        let completed = outcome.completed;
        let json = completed.then(|| std::fs::read_to_string(&brief.output).ok()).flatten();
        let _ = std::fs::remove_file(&brief.output);
        let edits = json.and_then(|json| domain::memory::parse_memory_edits(&json)).unwrap_or_default();
        let surface = gate::surface_of(request);
        let edits = gate::Gate { memories: self, project: &request.session.cwd, surface, stage: "finish" }.kept(edits);
        if edits.is_empty() {
            return None;
        }
        let lineage = reading::lineage(Some(reads));
        let by = Provenance { author: Author::Agent, now: now(), origin: brief.origin.clone(), lineage };
        let before = self.load(&request.session.cwd).memories;
        let applied = self.edit(&request.session.cwd, &edits, by);
        Some(applied.map(|listing| applied_since(&before, listing)))
    }
}

fn everything(listing: &MemoryListing) -> Vec<Memory> {
    listing.memories.iter().cloned().chain(listing.forgotten.iter().map(|f| f.memory.clone())).collect()
}

fn with_memory(request: &PromptRequest, brief: MemoryBrief) -> PromptRequest {
    request.clone().remembering(brief)
}

fn applied_since(before: &[Memory], listing: MemoryListing) -> MemoryApplied {
    let was = |id: MemoryId| before.iter().find(|memory| memory.id == id);
    let reworded =
        |memory: &&Memory| was(memory.id).is_some_and(|old| (&old.text, old.kind) != (&memory.text, memory.kind));
    MemoryApplied {
        added: listing.memories.iter().filter(|memory| was(memory.id).is_none()).count(),
        changed: listing.memories.iter().filter(reworded).count(),
        forgotten: before.iter().filter(|old| !listing.memories.iter().any(|memory| memory.id == old.id)).count(),
        listing,
    }
}

fn edits_file(run: RunId) -> PathBuf {
    let folder = std::env::temp_dir().join(concat!(brand::command!(), "-memory"));
    if let Err(error) = std::fs::create_dir_all(&folder) {
        tracing::warn!(%error, "could not create the memory edits folder");
    }
    folder.join(format!("{run}.json"))
}

pub fn now() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

#[cfg(test)]
mod anchors_tests;
#[cfg(test)]
mod tests;
