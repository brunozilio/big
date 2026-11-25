use std::path::Path;

use domain::PromptRequest;
use domain::memory::scorecard::RunOutcome;

use super::{Memories, MemoryShelf};

impl Memories {
    pub(super) fn holds_out(&self, project: &Path) -> bool {
        let mut held = false;
        let shelf = MemoryShelf::Repository(project.to_path_buf());
        if let Err(error) = self.update(&shelf, &mut |book| held = book.scorecard.next_held_out()) {
            tracing::warn!(%error, "could not count the request for the memory scorecard");
        }
        held
    }

    pub(super) fn keep_score(&self, request: &PromptRequest, held: bool, outcome: &RunOutcome) {
        if !domain::features::on(brand::features::MEMORY_SCORECARD) {
            return;
        }
        let shelf = MemoryShelf::Repository(request.session.cwd.clone());
        if let Err(error) = self.update(&shelf, &mut |book| book.scorecard.record(held, outcome)) {
            tracing::warn!(%error, "could not save the memory scorecard");
        }
    }
}
