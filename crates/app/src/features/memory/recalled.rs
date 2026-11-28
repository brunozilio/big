use domain::memory::{MemorySearches, SearchAnswer, recall, search};
use domain::{Memory, MemoryBrief, PromptRequest, RecallBudget, RunId};
use std::path::Path;

use super::{Memories, edits_file, now, team, with_memory};

fn personal(memories: &[Memory]) -> Vec<Memory> {
    memories.iter().filter(|memory| !domain::memory::team::is_team(memory.id)).cloned().collect()
}

impl Memories {
    pub fn find(&self, project: &Path, query: &str, limit: usize, searches: &mut MemorySearches) -> SearchAnswer {
        let shelves = self.shelves(project);
        let (kept, _) = self.known(&shelves);
        let known: Vec<Memory> = kept.into_iter().chain(team::load(&self.gate(project), &self.redactor)).collect();
        let scores = search(query, &known);
        let mut order: Vec<usize> = (0..known.len()).filter(|&i| scores[i] > 0.0).collect();
        order.sort_by(|&a, &b| scores[b].total_cmp(&scores[a]).then(a.cmp(&b)));
        let ranked: Vec<Memory> = order.into_iter().map(|i| known[i].clone()).collect();
        let answer = searches.answer(ranked, limit);
        if let SearchAnswer::Found(found) = &answer {
            self.mark_used(&shelves, &personal(found));
        }
        answer
    }

    pub fn brief(&self, run: RunId, request: &PromptRequest, tool: bool) -> PromptRequest {
        let Some(brief) = &request.host.memory else { return request.clone() };
        let project = &request.session.cwd;
        let shelves = self.shelves(project);
        let base = MemoryBrief {
            output: edits_file(run),
            origin: brief.origin.clone(),
            anchors: domain::features::on(brand::features::MEMORY_ANCHORS),
            structured: domain::features::on(brand::features::STRUCTURED_LEARNINGS),
            application: shelves.application.as_ref().map(|application| application.name.clone()),
            ..MemoryBrief::default()
        };
        if domain::features::on(brand::features::MEMORY_SCORECARD) && self.holds_out(project) {
            return with_memory(request, MemoryBrief { held_out: true, ..base });
        }
        let budget = if tool { RecallBudget::WITH_TOOL } else { RecallBudget::default() };
        let (kept, _) = self.known(&shelves);
        let held = kept.len();
        let known: Vec<Memory> = kept.into_iter().chain(team::load_in(project, &self.redactor)).collect();
        let recalled = recall(&request.prompt.text, &known, budget, now());
        self.mark_used(&shelves, &personal(&recalled));
        let anchored = if base.anchors {
            let unseen: Vec<Memory> =
                known.into_iter().filter(|memory| !recalled.iter().any(|r| r.id == memory.id)).collect();
            domain::memory::anchors::anchored(&unseen)
        } else {
            Vec::new()
        };
        let tool = tool && held > recalled.len();
        with_memory(request, MemoryBrief { recalled, tool, anchored, ..base })
    }
}
