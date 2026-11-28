use serde::{Deserialize, Serialize};

use super::{MemoryEdit, MemoryId};

const APPLICATION_FIRST_ID: u32 = 2_000_000;
const GLOBAL_FIRST_ID: u32 = 3_000_000;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryScope {
    #[default]
    Repository,
    Application,
    Global,
}

impl MemoryScope {
    pub const ALL: [MemoryScope; 3] = [MemoryScope::Repository, MemoryScope::Application, MemoryScope::Global];

    pub fn is_repository(&self) -> bool {
        *self == MemoryScope::Repository
    }

    fn first_id(self) -> u32 {
        match self {
            MemoryScope::Repository => 0,
            MemoryScope::Application => APPLICATION_FIRST_ID,
            MemoryScope::Global => GLOBAL_FIRST_ID,
        }
    }

    pub fn shown(self, id: MemoryId) -> MemoryId {
        MemoryId(id.0.saturating_add(self.first_id()))
    }

    pub fn of(id: MemoryId) -> (MemoryScope, MemoryId) {
        let scope = match id.0 {
            n if n >= GLOBAL_FIRST_ID => MemoryScope::Global,
            n if n >= APPLICATION_FIRST_ID => MemoryScope::Application,
            _ => MemoryScope::Repository,
        };
        (scope, MemoryId(id.0 - scope.first_id()))
    }

    pub fn label(id: MemoryId) -> Option<&'static str> {
        match Self::of(id).0 {
            MemoryScope::Repository => None,
            MemoryScope::Application => Some("application"),
            MemoryScope::Global => Some("global"),
        }
    }
}

impl MemoryEdit {
    pub fn routed(self) -> (MemoryScope, MemoryEdit) {
        let local = |id: MemoryId| MemoryScope::of(id);
        match self {
            MemoryEdit::Remember { kind, text, tags, touches, scope } => {
                (scope, MemoryEdit::Remember { kind, text, tags, touches, scope: MemoryScope::Repository })
            }
            MemoryEdit::Forget { id } => {
                let (scope, id) = local(id);
                (scope, MemoryEdit::Forget { id })
            }
            MemoryEdit::Restore { id } => {
                let (scope, id) = local(id);
                (scope, MemoryEdit::Restore { id })
            }
            MemoryEdit::Revise { id, text } => {
                let (scope, id) = local(id);
                (scope, MemoryEdit::Revise { id, text })
            }
            MemoryEdit::Reclassify { id, kind } => {
                let (scope, id) = local(id);
                (scope, MemoryEdit::Reclassify { id, kind })
            }
            MemoryEdit::Purge { id } => {
                let (scope, id) = local(id);
                (scope, MemoryEdit::Purge { id })
            }
            MemoryEdit::PurgeSource { source } => (MemoryScope::Repository, MemoryEdit::PurgeSource { source }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::MemoryKind;

    #[test]
    fn an_id_shown_to_the_agent_says_which_book_it_came_from() {
        for scope in MemoryScope::ALL {
            assert_eq!(MemoryScope::of(scope.shown(MemoryId(7))), (scope, MemoryId(7)));
        }
        assert_eq!(MemoryScope::of(MemoryId(1_000_003)), (MemoryScope::Repository, MemoryId(1_000_003)), "team");
        assert_eq!(MemoryScope::label(MemoryScope::Global.shown(MemoryId(1))), Some("global"));
        assert_eq!(MemoryScope::label(MemoryId(1)), None);
    }

    #[test]
    fn an_edit_goes_to_the_book_its_id_or_scope_names() {
        let forget = MemoryEdit::Forget { id: MemoryScope::Application.shown(MemoryId(4)) };
        assert_eq!(forget.routed(), (MemoryScope::Application, MemoryEdit::Forget { id: MemoryId(4) }));
        let remember = |scope| MemoryEdit::Remember {
            kind: MemoryKind::Semantic,
            text: "prefers short commits".into(),
            tags: Vec::new(),
            touches: Vec::new(),
            scope,
        };
        assert_eq!(remember(MemoryScope::Global).routed(), (MemoryScope::Global, remember(MemoryScope::Repository)));
    }
}
