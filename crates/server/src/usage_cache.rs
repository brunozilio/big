use std::collections::HashMap;

use domain::EngineEvent;
use domain::catalog::Listed;
use domain::conversation::SessionStatus;
use domain::trace::Record;

pub type Spent = Vec<(u64, EngineEvent)>;

#[derive(Debug)]
struct Kept {
    updated: u64,
    spent: Spent,
}

#[derive(Debug, Default)]
pub struct UsageCache {
    kept: HashMap<String, Kept>,
}

impl UsageCache {
    pub fn keep_only(&mut self, listed: &[Listed]) {
        self.kept.retain(|session, _| listed.iter().any(|row| row.session.0 == *session));
    }

    pub fn spent(&mut self, listed: &Listed, read: impl FnOnce() -> Option<Vec<Record>>) -> &[(u64, EngineEvent)] {
        let id = &listed.session.0;
        let fresh = self.kept.get(id).is_some_and(|kept| kept.updated == listed.updated) && settled(listed.status);
        if !fresh {
            match read() {
                Some(records) => {
                    self.kept.insert(id.clone(), Kept { updated: listed.updated, spent: spent_in(records) });
                }
                None => {
                    self.kept.remove(id);
                }
            }
        }
        self.kept.get(id).map_or(&[], |kept| kept.spent.as_slice())
    }
}

fn settled(status: SessionStatus) -> bool {
    !matches!(status, SessionStatus::Running | SessionStatus::Thinking)
}

fn spends(event: &EngineEvent) -> bool {
    match event {
        EngineEvent::Usage(_) => true,
        EngineEvent::AgentEvent { event, .. } => spends(event),
        _ => false,
    }
}

fn spent_in(records: Vec<Record>) -> Spent {
    records
        .into_iter()
        .filter_map(|record| match record {
            Record::Event { at_ms, mut event } if spends(&event) => {
                crate::usage::priced(&mut event);
                Some((at_ms, event))
            }
            _ => None,
        })
        .collect()
}

#[cfg(test)]
#[path = "usage_cache_tests.rs"]
mod tests;
