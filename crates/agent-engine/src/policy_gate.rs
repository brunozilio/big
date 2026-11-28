use domain::memory::MemoryPolicy;
use domain::permission::{BLOCKED, Call, Decision, Effect, Layer, MemoryAccess, PolicyEngine, Surface, Verdict};

const MEMORY_FILE: &str = "safety/memory.ts";
const MEMORY_EXPORT: &str = "memory";

#[derive(Debug, Clone, Copy)]
pub struct Asked<'a> {
    pub surface: Surface,
    pub stage: &'a str,
    pub owner: &'a str,
}

pub fn decide(asked: Asked<'_>, call: &Call<'_>) -> Verdict {
    let allowed = domain::permission::allowed::global();
    settled(asked, call.tool, PolicyEngine::unattended(call, allowed.as_ref()))
}

pub fn memory(asked: Asked<'_>, tool: &str, access: MemoryAccess, application: Option<&str>) -> Verdict {
    memory_under(&memory_policy(), asked, tool, access, application)
}

pub fn memory_under(
    policy: &MemoryPolicy,
    asked: Asked<'_>,
    tool: &str,
    access: MemoryAccess,
    application: Option<&str>,
) -> Verdict {
    let effects = [Effect::Memory(access)];
    let listed = decide(asked, &Call { tool, subject: access.word(), effects: &effects });
    if listed != Verdict::Allow {
        return listed;
    }
    match policy.refusal(asked.surface, access, application) {
        Some(why) => settled(asked, tool, Decision::deny(format!("{tool} is blocked because {why}"), Layer::Memory)),
        None => Verdict::Allow,
    }
}

pub fn memory_policy() -> MemoryPolicy {
    MemoryPolicy::of(content::value(MEMORY_FILE, MEMORY_EXPORT))
}

fn settled(asked: Asked<'_>, tool: &str, decision: Decision) -> Verdict {
    if let (Verdict::Deny(reason), Some(layer)) = (&decision.verdict, decision.layer) {
        blocked(asked, tool, reason, layer);
    }
    decision.verdict
}

pub fn blocked(asked: Asked<'_>, tool: &str, reason: &str, layer: Layer) {
    tracing::warn!(
        target: "audit",
        surface = asked.surface.word(),
        stage = asked.stage,
        user = asked.owner,
        tool,
        reason,
        layer = layer.word(),
        "policy.blocked"
    );
}

pub fn unasked(asked: Asked<'_>, tool: &str) -> String {
    let reason = format!("{tool} asks a person first, and {} cannot ask anyone yet.", asked.surface.word());
    blocked(asked, tool, &reason, Layer::List);
    refusal(&reason)
}

pub fn refusal(reason: &str) -> String {
    format!("{BLOCKED}: {reason}")
}

#[cfg(test)]
mod tests {
    use domain::memory::MemoryScope;
    use serde_json::json;

    use super::*;

    const ASKED: Asked<'static> = Asked { surface: Surface::Mcp, stage: "tools/call", owner: "local" };

    #[test]
    fn the_shipped_memory_policy_keeps_every_surface_reading_and_writing() {
        let policy = MemoryPolicy::of(content::value_in(None, MEMORY_FILE, MEMORY_EXPORT));
        for surface in Surface::ALL {
            for access in [MemoryAccess::Read, MemoryAccess::Write(MemoryScope::Global)] {
                assert_eq!(policy.refusal(surface, access, None), None, "{surface:?} {access:?}");
            }
        }
    }

    #[test]
    fn a_memory_policy_that_refuses_the_scope_blocks_with_its_reason() {
        let policy = MemoryPolicy::of(Some(json!({ "write": { "mcp": ["repository"] } })));
        let access = MemoryAccess::Write(MemoryScope::Global);
        let Verdict::Deny(reason) = memory_under(&policy, ASKED, "memory_remember", access, None) else {
            panic!("the global scope is not the mcp's to write");
        };
        assert!(reason.contains("global scope"), "{reason}");
        let fine = MemoryAccess::Write(MemoryScope::Repository);
        assert_eq!(memory_under(&policy, ASKED, "memory_remember", fine, None), Verdict::Allow);
    }

    #[test]
    fn a_refusal_names_the_policy_and_a_missing_person() {
        let told = unasked(ASKED, "x_tracker_create");
        assert!(told.starts_with("blocked_by_policy: x_tracker_create asks a person first"), "{told}");
    }
}
