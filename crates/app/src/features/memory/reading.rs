use std::sync::Arc;

use domain::Memory;
use domain::label::{Levels, Mark, Read, Reads, Trust};
use domain::memory::Lineage;
use domain::permission::Surface;
use platform::sync::Locked;

pub fn fresh(reads: &[Read]) -> Reads {
    Arc::new(std::sync::Mutex::new(reads.to_vec()))
}

pub(crate) struct Noted<'a> {
    pub surface: Surface,
    pub stage: &'a str,
}

impl Noted<'_> {
    pub(crate) fn read(&self, reads: Option<&Reads>, memories: &[Memory]) {
        let levels = crate::labels::levels();
        for memory in memories {
            let Some(read) = memory.read(&levels) else { continue };
            if read.trust == Trust::External {
                self.told(&levels, memory, &read);
            }
            if let Some(reads) = reads {
                let mut held = reads.locked();
                if !held.contains(&read) {
                    held.push(read);
                }
            }
        }
    }

    fn told(&self, levels: &Levels, memory: &Memory, read: &Read) {
        let sources = memory.lineage.as_ref().map(|lineage| lineage.sources.join(", ")).unwrap_or_default();
        tracing::warn!(
            target: "audit",
            surface = self.surface.word(),
            stage = self.stage,
            memory = memory.id.0,
            source = read.source.as_str(),
            from = sources.as_str(),
            mode = crate::labels::mode(levels).word(),
            "memory.untrusted"
        );
    }
}

pub(crate) fn lineage(reads: Option<&Reads>) -> Option<Lineage> {
    let reads = reads?.locked().clone();
    Some(Lineage::of(&Mark::of(&crate::labels::levels(), &reads)))
}

pub(crate) fn purged(memories: &[Memory], left: &[Memory]) {
    for gone in memories.iter().filter(|memory| !left.iter().any(|kept| kept.id == memory.id)) {
        let sources = gone.lineage.as_ref().map(|lineage| lineage.sources.join(", ")).unwrap_or_default();
        let trust = gone.lineage.as_ref().map_or("unknown", |lineage| lineage.trust.word());
        tracing::warn!(target: "audit", memory = gone.id.0, trust, from = sources.as_str(), "memory.purged");
    }
}
