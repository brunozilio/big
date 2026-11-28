use super::allowed::Behavior;
use super::{Allowed, Call, Effect, PolicyContext, PolicyEngine, Reason, Verdict};
use crate::PermissionMode;
use crate::memory::MemoryScope;

pub const BLOCKED: &str = "blocked_by_policy";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Surface {
    Chat,
    Workflow,
    Routine,
    Mcp,
}

impl Surface {
    pub const ALL: [Self; 4] = [Self::Chat, Self::Workflow, Self::Routine, Self::Mcp];

    pub fn word(self) -> &'static str {
        match self {
            Self::Chat => "chat",
            Self::Workflow => "workflow",
            Self::Routine => "routine",
            Self::Mcp => "mcp",
        }
    }

    pub fn named(word: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|surface| surface.word() == word.trim())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layer {
    List,
    Declared,
    Bounds,
    Floor,
    Mode,
    Memory,
}

impl Layer {
    pub fn word(self) -> &'static str {
        match self {
            Self::List => "list",
            Self::Declared => "declared",
            Self::Bounds => "bounds",
            Self::Floor => "floor",
            Self::Mode => "mode",
            Self::Memory => "memory",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryAccess {
    Read,
    Write(MemoryScope),
}

impl MemoryAccess {
    pub fn word(self) -> &'static str {
        match self {
            Self::Read => "memory.read",
            Self::Write(_) => "memory.write",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Decision {
    pub verdict: Verdict,
    pub layer: Option<Layer>,
}

impl Decision {
    pub fn allow() -> Self {
        Self { verdict: Verdict::Allow, layer: None }
    }

    pub fn deny(reason: String, layer: Layer) -> Self {
        Self { verdict: Verdict::Deny(reason), layer: Some(layer) }
    }
}

pub fn listed(call: &Call<'_>, allowed: &Allowed) -> Option<Verdict> {
    match allowed.verdict(call) {
        Some(Behavior::Deny) => Some(Verdict::Deny(format!("{} is denied by the permission list.", call.tool))),
        Some(Behavior::Ask) => Some(Verdict::Ask { reason: Reason::Listed, rememberable: false }),
        _ => None,
    }
}

impl PolicyEngine {
    pub fn unattended(call: &Call<'_>, allowed: Option<&Allowed>) -> Decision {
        match allowed.and_then(|allowed| listed(call, allowed)) {
            Some(verdict) => Decision { verdict, layer: Some(Layer::List) },
            None => Decision::allow(),
        }
    }

    pub fn layer(call: &Call<'_>, cx: &PolicyContext<'_>) -> Layer {
        if cx.allowed.is_some_and(|allowed| allowed.verdict(call) == Some(Behavior::Deny)) {
            return Layer::List;
        }
        if cx.bounds.hard.verdict(call).is_some_and(|verdict| verdict.denies()) {
            return Layer::Floor;
        }
        let read_only = call.effects.iter().any(|effect| match effect {
            Effect::WritePath(place) => cx.bounds.read_only_holding(&place.real).is_some(),
            _ => false,
        });
        if read_only {
            return Layer::Bounds;
        }
        let refused =
            call.effects.iter().any(|effect| matches!(effect, Effect::Operation(operation) if operation.refused()));
        if refused && cx.mode() != PermissionMode::Plan {
            return Layer::Declared;
        }
        Layer::Mode
    }
}

#[cfg(test)]
mod tests {
    use super::super::{Bounds, Grant, Remembered, Rule};
    use super::*;

    fn call(tool: &str) -> Call<'_> {
        Call { tool, subject: "", effects: &[] }
    }

    #[test]
    fn unattended_surfaces_follow_only_what_the_list_says_and_run_the_rest_as_before() {
        let list = Allowed::of([
            Rule::new(Grant::Tool("mcp__c__x_code_merge".into()), Behavior::Deny),
            Rule::new(Grant::Tool("mcp__c__x_tracker_comment".into()), Behavior::Ask),
        ]);
        let denied = PolicyEngine::unattended(&call("mcp__c__x_code_merge"), Some(&list));
        assert_eq!(denied.layer, Some(Layer::List));
        assert!(denied.verdict.denies());
        let asked = PolicyEngine::unattended(&call("mcp__c__x_tracker_comment"), Some(&list));
        assert_eq!(asked.verdict, Verdict::Ask { reason: Reason::Listed, rememberable: false });
        assert_eq!(PolicyEngine::unattended(&call("mcp__c__x_tracker_issue"), Some(&list)), Decision::allow());
        assert_eq!(PolicyEngine::unattended(&call("mcp__c__x_code_merge"), None), Decision::allow());
    }

    #[test]
    fn the_chat_names_the_layer_that_denied() {
        let list = Allowed::of([Rule::new(Grant::Tool("mcp__c__x_code_merge".into()), Behavior::Deny)]);
        let (remembered, bounds) = (Remembered::default(), Bounds::default());
        let cx = PolicyContext {
            mode: PermissionMode::Auto,
            ceiling: None,
            remembered: &remembered,
            bounds: &bounds,
            no_prompts: false,
            allowed: Some(&list),
        };
        let merge = call("mcp__c__x_code_merge");
        assert!(PolicyEngine::decide(&merge, &cx).denies());
        assert_eq!(PolicyEngine::layer(&merge, &cx), Layer::List);
        assert_eq!(PolicyEngine::layer(&call("Other"), &cx), Layer::Mode);
        assert_eq!(Surface::named(" routine "), Some(Surface::Routine));
    }
}
