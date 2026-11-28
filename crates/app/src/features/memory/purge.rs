use domain::{Memory, MemoryEdit};

use crate::api::memory::MemoryListing;

fn everything(listing: &MemoryListing) -> impl Iterator<Item = &Memory> {
    listing.memories.iter().chain(listing.forgotten.iter().map(|forgotten| &forgotten.memory))
}

fn asked(edit: &MemoryEdit, memory: &Memory) -> bool {
    match edit {
        MemoryEdit::Purge { id } => memory.id == *id,
        MemoryEdit::PurgeSource { source } => memory.lineage.as_ref().is_some_and(|lineage| lineage.names(source)),
        _ => false,
    }
}

pub(super) fn targeted(listing: &MemoryListing, edits: &[MemoryEdit]) -> Vec<Memory> {
    everything(listing).filter(|memory| edits.iter().any(|edit| asked(edit, memory))).cloned().collect()
}

pub(super) fn told(targeted: &[Memory], after: &MemoryListing) {
    for gone in targeted.iter().filter(|memory| !everything(after).any(|kept| kept.id == memory.id)) {
        let sources = gone.lineage.as_ref().map(|lineage| lineage.sources.join(", ")).unwrap_or_default();
        let trust = gone.lineage.as_ref().map_or("unknown", |lineage| lineage.trust.word());
        tracing::warn!(target: "audit", memory = gone.id.0, trust, from = sources.as_str(), "memory.purged");
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use domain::label::Trust;
    use domain::memory::Lineage;
    use domain::{Author, Forgotten, MemoryId, MemoryKind};

    use super::*;

    fn memory(id: u32, sources: &[&str]) -> Memory {
        let lineage = (!sources.is_empty()).then(|| Lineage {
            trust: Trust::External,
            sources: sources.iter().map(|source| (*source).to_owned()).collect(),
            label: None,
        });
        Memory {
            id: MemoryId(id),
            kind: MemoryKind::Semantic,
            text: format!("m{id}"),
            tags: Vec::new(),
            created: 0,
            origin: None,
            author: Author::Agent,
            history: Vec::new(),
            uses: 0,
            last_used: 0,
            touches: Vec::new(),
            credit: None,
            lineage,
        }
    }

    #[test]
    fn only_what_the_purge_asked_for_is_told_as_purged() {
        let before = MemoryListing {
            path: PathBuf::new(),
            memories: vec![memory(0, &["Mail"]), memory(1, &[])],
            forgotten: vec![
                Forgotten { memory: memory(2, &[]), at: 0 },
                Forgotten { memory: memory(3, &["Mail"]), at: 0 },
            ],
        };
        let targeted = targeted(&before, &[MemoryEdit::PurgeSource { source: "Mail".into() }]);
        assert_eq!(targeted.iter().map(|memory| memory.id.0).collect::<Vec<_>>(), [0, 3]);
        let one = super::targeted(&before, &[MemoryEdit::Purge { id: MemoryId(1) }]);
        assert_eq!(one.iter().map(|memory| memory.id.0).collect::<Vec<_>>(), [1]);
        assert!(super::targeted(&before, &[MemoryEdit::Forget { id: MemoryId(2) }]).is_empty());
    }
}
