use std::sync::Arc;

use crate::api::Unsolicited;
use domain::memory::MemorySearches;
use domain::{PromptRequest, RunId};

use super::mcp::MemoryEndpoint;
use crate::features::context::{Context, Finish};

pub fn prepare(context: &Context<'_>, run: RunId, request: PromptRequest) -> (PromptRequest, Option<Finish>) {
    if request.host.memory.is_none() {
        return (request, None);
    }
    let memories = super::of(context.services);
    let wanted = domain::features::on(brand::features::MEMORY_SEARCH);
    let mut request = memories.brief(run, &request, wanted);
    let surface = super::gate::surface_of(&request);
    let reads = super::reading::fresh(&request.host.reads);
    if let Some(brief) = &request.host.memory {
        super::reading::Noted { surface, stage: "brief" }.read(Some(&reads), &brief.recalled);
    }
    if let Some(brief) = request.host.memory.as_ref().filter(|brief| brief.tool) {
        let first = request.session.resume.is_none();
        let shown = if first { brief.recalled.iter().map(|m| m.id).collect() } else { Vec::new() };
        let searches = MemorySearches::new(shown);
        let project = request.session.cwd.clone();
        let endpoint = MemoryEndpoint::for_run(memories.clone(), project, searches, surface, Arc::clone(&reads));
        request.host.credentials.mcp.push(endpoint);
    }
    let kept = request.clone();
    let finish: Finish = Box::new(move |outcome| {
        Some(match memories.finish(&kept, outcome, &reads)? {
            Ok(applied) => Unsolicited::MemoryApplied(applied),
            Err(message) => Unsolicited::MemoryFailed { path: kept.session.cwd.clone(), message },
        })
    });
    (request, Some(finish))
}
