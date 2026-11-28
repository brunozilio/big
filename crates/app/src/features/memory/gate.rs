use std::path::Path;

use agent_engine::policy_gate::{self, Asked};
use domain::memory::{MEMORY_SEARCH_TOOL, MEMORY_SERVER, MemoryPolicy};
use domain::permission::{Layer, MemoryAccess, Surface, Verdict};
use domain::{MemoryEdit, PromptRequest};

use super::Memories;

pub(crate) const REMEMBER: &str = "memory_remember";

pub(crate) fn surface_of(request: &PromptRequest) -> Surface {
    if request.host.attribution.item.is_some() { Surface::Workflow } else { Surface::Chat }
}

pub(crate) fn access_of(edit: &MemoryEdit) -> MemoryAccess {
    MemoryAccess::Write(edit.clone().routed().0)
}

pub(crate) fn tool_of(access: MemoryAccess) -> &'static str {
    match access {
        MemoryAccess::Read => MEMORY_SEARCH_TOOL,
        MemoryAccess::Write(_) => REMEMBER,
    }
}

pub(crate) struct Gate<'a> {
    pub memories: &'a Memories,
    pub project: &'a Path,
    pub surface: Surface,
    pub stage: &'a str,
}

impl Gate<'_> {
    fn application(&self) -> Option<String> {
        self.memories.application_of(self.project).map(|application| application.name)
    }

    fn asked(&self) -> Asked<'_> {
        Asked { surface: self.surface, stage: self.stage, owner: domain::principal::LOCAL_USER, operation: None }
    }

    pub(crate) fn refused(&self, access: MemoryAccess) -> Option<String> {
        self.refused_under(&policy_gate::memory_policy(), access)
    }

    pub(crate) fn refused_under(&self, policy: &MemoryPolicy, access: MemoryAccess) -> Option<String> {
        let why = policy.refusal(self.surface, access, self.application().as_deref())?;
        let tool = tool_of(access);
        let reason = format!("{tool} is blocked because {why}");
        policy_gate::blocked(self.asked(), tool, &reason, Layer::Memory);
        Some(policy_gate::refusal(&reason))
    }

    pub(crate) fn listed(&self, tool: &str, access: MemoryAccess) -> Option<String> {
        let served = domain::mcp::served_name(MEMORY_SERVER, tool);
        let application = self.application();
        match policy_gate::memory(self.asked(), &served, access, application.as_deref()) {
            Verdict::Allow => None,
            Verdict::Deny(reason) => Some(policy_gate::refusal(&reason)),
            Verdict::Ask { .. } => Some(policy_gate::unasked(self.asked(), &served)),
        }
    }

    pub(crate) fn kept(&self, edits: Vec<MemoryEdit>) -> Vec<MemoryEdit> {
        let policy = policy_gate::memory_policy();
        edits.into_iter().filter(|edit| self.refused_under(&policy, access_of(edit)).is_none()).collect()
    }
}
