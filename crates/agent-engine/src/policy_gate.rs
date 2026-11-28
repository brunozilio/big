use std::sync::{Mutex, PoisonError};

use domain::memory::MemoryPolicy;
use domain::permission::hard::Floor;
use domain::permission::{BLOCKED, Call, Decision, Effect, Layer, MemoryAccess, PolicyEngine, Surface, Verdict};
use serde_json::Value;

const MEMORY_FILE: &str = "safety/memory.ts";
const MEMORY_EXPORT: &str = "memory";

#[derive(Debug, Clone, Copy)]
pub struct Asked<'a> {
    pub surface: Surface,
    pub stage: &'a str,
    pub owner: &'a str,
}

static MEMORY_POLICY: Mutex<Option<(String, MemoryPolicy)>> = Mutex::new(None);

pub fn decide(asked: Asked<'_>, call: &Call<'_>) -> Verdict {
    decide_under(&crate::tools::hard_floor::floor(None), asked, call)
}

pub fn decide_under(floor: &Floor, asked: Asked<'_>, call: &Call<'_>) -> Verdict {
    let allowed = domain::permission::allowed::global();
    let listed = PolicyEngine::unattended(call, allowed.as_ref());
    settled(asked, call.tool, floored(floor, call, listed))
}

fn floored(floor: &Floor, call: &Call<'_>, listed: Decision) -> Decision {
    let tightened = floor.tightened(call, listed.verdict.clone());
    if tightened == listed.verdict {
        return listed;
    }
    match tightened.without_the_user(call.tool) {
        Verdict::Deny(reason) => Decision::deny(reason, Layer::Floor),
        verdict => Decision { verdict, layer: Some(Layer::Floor) },
    }
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
    let folder = content::folder();
    let source = content::user_text(folder.as_deref(), MEMORY_FILE).or_else(|| content::builtin_text(MEMORY_FILE));
    let Some(source) = source else { return MemoryPolicy::default() };
    let mut kept = MEMORY_POLICY.lock().unwrap_or_else(PoisonError::into_inner);
    if let Some((_, policy)) = kept.as_ref().filter(|(known, _)| *known == source) {
        return policy.clone();
    }
    let policy = policy_of(&source);
    *kept = Some((source, policy.clone()));
    policy
}

pub fn policy_of(source: &str) -> MemoryPolicy {
    match script::exported(MEMORY_FILE, source, MEMORY_EXPORT) {
        Ok(Value::Null) => MemoryPolicy::default(),
        Ok(value) => MemoryPolicy::of(Some(value)),
        Err(error) => {
            tracing::warn!(%error, file = MEMORY_FILE, "the memory policy did not load, so memory stays closed");
            MemoryPolicy::refusing()
        }
    }
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
        outcome = "denied",
        "policy.blocked"
    );
}

pub fn unasked(asked: Asked<'_>, tool: &str) -> String {
    let reason = format!("{tool} asks a person first, and {} cannot ask anyone yet.", asked.surface.word());
    blocked(asked, tool, &reason, Layer::List);
    refusal(&reason)
}

pub fn subject(action: &str, args: &Value) -> String {
    let shown = crate::mcp::summary_of(args);
    if shown.is_empty() { action.to_owned() } else { format!("{action} {shown}") }
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
        let policy = policy_of(&content::builtin_text(MEMORY_FILE).expect("shipped"));
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

    #[test]
    fn a_missing_or_null_export_opens_memory_and_a_broken_script_closes_it() {
        let write = MemoryAccess::Write(MemoryScope::Repository);
        for open in ["export const other = 1;", "export const memory = null;"] {
            assert_eq!(policy_of(open).refusal(Surface::Mcp, write, None), None, "{open}");
        }
        let broken = policy_of("export const memory = {;");
        assert!(broken.refusal(Surface::Mcp, write, None).is_some());
        assert!(broken.refusal(Surface::Chat, MemoryAccess::Read, None).is_some());
        let thrown = policy_of("throw new Error('no');");
        assert!(thrown.refusal(Surface::Workflow, MemoryAccess::Read, None).is_some());
    }

    #[test]
    fn the_floor_refuses_what_it_would_ask_a_person_for_when_no_one_is_there() {
        let floor = Floor::default();
        let effects: [Effect; 0] = [];
        let hidden = Call { tool: "x_tracker_comment", subject: "tracker.comment ok\u{202E}exe", effects: &effects };
        let Verdict::Deny(reason) = decide_under(&floor, ASKED, &hidden) else {
            panic!("a hidden character is never sent unattended");
        };
        assert!(reason.contains("safety floor"), "{reason}");
        let plain = Call { tool: "x_code_merge", subject: "code.merge 7", effects: &effects };
        assert_eq!(decide_under(&floor, ASKED, &plain), Verdict::Allow);
    }
}
