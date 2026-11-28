use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

pub const MAX_MEMORY_CHARS: usize = 600;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryKind {
    Episodic,
    Semantic,
    Procedural,
}

impl MemoryKind {
    pub const ALL: [MemoryKind; 3] = [MemoryKind::Episodic, MemoryKind::Semantic, MemoryKind::Procedural];

    pub fn key(self) -> &'static str {
        match self {
            MemoryKind::Episodic => "episodic",
            MemoryKind::Semantic => "semantic",
            MemoryKind::Procedural => "procedural",
        }
    }

    pub(crate) fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.key().eq_ignore_ascii_case(key.trim()))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Author {
    Agent,
    User,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct MemoryId(pub u32);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Memory {
    pub id: MemoryId,
    pub kind: MemoryKind,
    pub text: String,
    #[serde(default)]
    pub tags: Vec<String>,
    pub created: i64,
    #[serde(default)]
    pub origin: Option<String>,
    pub author: Author,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub history: Vec<Revision>,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub uses: u32,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub last_used: i64,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub touches: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub credit: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lineage: Option<Lineage>,
}

fn is_zero<T: Default + PartialEq>(value: &T) -> bool {
    *value == T::default()
}

pub const REVISIONS_KEPT: usize = 5;
pub const FORGOTTEN_KEPT_DAYS: i64 = 30;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Revision {
    pub text: String,
    pub until: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Forgotten {
    pub memory: Memory,
    pub at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum MemoryEdit {
    Remember {
        kind: MemoryKind,
        text: String,
        tags: Vec<String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        touches: Vec<String>,
        #[serde(default, skip_serializing_if = "MemoryScope::is_repository")]
        scope: MemoryScope,
    },
    Forget {
        id: MemoryId,
    },
    Revise {
        id: MemoryId,
        text: String,
    },
    Reclassify {
        id: MemoryId,
        kind: MemoryKind,
    },
    Restore {
        id: MemoryId,
    },
    Purge {
        id: MemoryId,
    },
    PurgeSource {
        source: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Provenance {
    pub author: Author,
    pub now: i64,
    pub origin: Option<String>,
    pub lineage: Option<Lineage>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct MemoryBook {
    #[serde(default)]
    pub next: u32,
    #[serde(default)]
    pub memories: Vec<Memory>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub forgotten: Vec<Forgotten>,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub scorecard: scorecard::Scorecard,
}

impl MemoryBook {
    pub fn holds_more_than(&self, recalled: &[Memory]) -> bool {
        self.memories.len() > recalled.len()
    }

    pub fn apply(&mut self, edits: &[MemoryEdit], by: &Provenance) -> Vec<MemoryId> {
        let expiry = by.now - FORGOTTEN_KEPT_DAYS * 24 * 60 * 60;
        self.forgotten.retain(|f| f.at >= expiry);
        let mut touched = Vec::new();
        for edit in edits {
            let changed = match edit {
                MemoryEdit::Remember { kind, text, tags, touches, .. } => self.remember(*kind, text, tags, touches, by),
                MemoryEdit::Forget { id } => {
                    self.forget(*id, by.now);
                    None
                }
                MemoryEdit::Restore { id } => self.restore(*id),
                MemoryEdit::Reclassify { id, kind } => self.reclassify(*id, *kind),
                MemoryEdit::Revise { id, text } => self.revise(*id, text, by),
                MemoryEdit::Purge { id } => {
                    self.purge(|memory| memory.id == *id);
                    None
                }
                MemoryEdit::PurgeSource { source } => {
                    self.purge(|memory| memory.lineage.as_ref().is_some_and(|lineage| lineage.names(source)));
                    None
                }
            };
            touched.extend(changed);
        }
        touched
    }

    fn remember(
        &mut self,
        kind: MemoryKind,
        text: &str,
        tags: &[String],
        touches: &[String],
        by: &Provenance,
    ) -> Option<MemoryId> {
        let text = clean_text(text)?;
        let touches = anchors::clean(touches);
        if let Some(same) = self.memories.iter_mut().find(|m| m.kind == kind && same_text(&m.text, &text)) {
            return anchors::widen(&mut same.touches, &touches).then_some(same.id);
        }
        let id = MemoryId(self.next.max(self.after_highest()));
        self.next = id.0 + 1;
        self.memories.push(Memory {
            id,
            kind,
            text,
            tags: clean_tags(tags),
            created: by.now,
            origin: by.origin.clone(),
            author: by.author,
            history: Vec::new(),
            uses: 0,
            last_used: 0,
            touches,
            credit: None,
            lineage: by.lineage.clone(),
        });
        Some(id)
    }

    fn forget(&mut self, id: MemoryId, now: i64) {
        if let Some(at) = self.memories.iter().position(|m| m.id == id) {
            let memory = self.memories.remove(at);
            self.forgotten.push(Forgotten { memory, at: now });
        }
    }

    fn restore(&mut self, id: MemoryId) -> Option<MemoryId> {
        let at = self.forgotten.iter().position(|f| f.memory.id == id)?;
        let memory = self.forgotten.remove(at).memory;
        let place = self.memories.partition_point(|m| m.id < memory.id);
        self.memories.insert(place, memory);
        Some(id)
    }

    fn reclassify(&mut self, id: MemoryId, kind: MemoryKind) -> Option<MemoryId> {
        let memory = self.memories.iter_mut().find(|m| m.id == id && m.kind != kind)?;
        memory.kind = kind;
        Some(id)
    }

    fn revise(&mut self, id: MemoryId, text: &str, by: &Provenance) -> Option<MemoryId> {
        let text = clean_text(text)?;
        let memory = self.memories.iter_mut().find(|m| m.id == id)?;
        let old = std::mem::replace(&mut memory.text, text);
        memory.history.push(Revision { text: old, until: by.now });
        let extra = memory.history.len().saturating_sub(REVISIONS_KEPT);
        memory.history.drain(..extra);
        if let Some(lineage) = by.lineage.as_ref().filter(|lineage| lineage::replaces(lineage, memory)) {
            memory.lineage = Some(lineage.clone());
        }
        Some(id)
    }

    fn purge(&mut self, gone: impl Fn(&Memory) -> bool) {
        self.memories.retain(|memory| !gone(memory));
        self.forgotten.retain(|forgotten| !gone(&forgotten.memory));
    }

    pub fn take(&mut self, id: MemoryId) -> Option<Memory> {
        let at = self.memories.iter().position(|m| m.id == id)?;
        Some(self.memories.remove(at))
    }

    pub fn adopt(&mut self, memory: Memory) -> MemoryId {
        let id = MemoryId(self.next.max(self.after_highest()));
        self.next = id.0 + 1;
        self.memories.push(Memory { id, ..memory });
        id
    }

    pub fn mark_used(&mut self, ids: &[MemoryId], now: i64) {
        for memory in self.memories.iter_mut().filter(|m| ids.contains(&m.id)) {
            memory.uses = memory.uses.saturating_add(1);
            memory.last_used = memory.last_used.max(now);
        }
    }

    fn after_highest(&self) -> u32 {
        let forgotten = self.forgotten.iter().map(|f| &f.memory);
        self.memories.iter().chain(forgotten).map(|m| m.id.0 + 1).max().unwrap_or(0)
    }
}

fn clean_text(text: &str) -> Option<String> {
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if text.is_empty() {
        return None;
    }
    if text.chars().count() <= MAX_MEMORY_CHARS {
        return Some(text);
    }
    let cut: String = text.chars().take(MAX_MEMORY_CHARS).collect();
    let cut = cut.rsplit_once(' ').map_or(cut.as_str(), |(head, _)| head);
    Some(format!("{cut}…"))
}

fn same_text(a: &str, b: &str) -> bool {
    a.to_lowercase() == b.to_lowercase()
}

fn clean_tags(tags: &[String]) -> Vec<String> {
    let mut seen = BTreeSet::new();
    tags.iter()
        .map(|tag| tag.trim().trim_start_matches('#').to_lowercase())
        .filter(|tag| !tag.is_empty() && seen.insert(tag.clone()))
        .take(8)
        .collect()
}

pub mod anchors;
mod brief;
pub mod check;
pub mod decision_note;
pub mod lineage;
mod optimize;
mod policy;
mod recall;
mod sanitize;
mod scope;
pub mod scorecard;
pub mod search;
mod searches;
pub mod seed;
pub mod team;

pub use brief::{MEMORY_SEARCH_TOOL, MEMORY_SERVER, MemoryBrief, parse_memory_edits};
pub use lineage::Lineage;
pub use optimize::optimize_prompt;
pub use policy::{MemoryPolicy, MemoryRules};
pub use recall::{RecallBudget, links, recall};
pub use sanitize::{REDACTED, Sanitized, sanitize};
pub use scope::MemoryScope;
pub use search::search;
pub use searches::{MemorySearches, SearchAnswer};

#[cfg(test)]
mod lifecycle_tests;
#[cfg(test)]
mod searches_tests;
#[cfg(test)]
mod tests;
