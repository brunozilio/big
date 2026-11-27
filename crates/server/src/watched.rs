use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use app::EngineEvent;
use domain::trace::HistoryStamp;

use crate::marks::now_ms;
use crate::session_view::Shown;

const NOTIFY_GAP: Duration = Duration::from_millis(300);
const KEPT_AFTER_END: Duration = Duration::from_secs(5);
const READ_AT_MOST: usize = 64;

struct Watching {
    shown: Shown,
    told: Option<Instant>,
    ended: Option<Instant>,
}

#[derive(Default)]
pub struct Watched {
    runs: Mutex<HashMap<String, Watching>>,
    read: Mutex<HashMap<String, (HistoryStamp, Arc<Shown>)>>,
}

impl std::fmt::Debug for Watched {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_struct("Watched").finish_non_exhaustive()
    }
}

fn streams(event: &EngineEvent) -> bool {
    match event {
        EngineEvent::TextDelta(_) | EngineEvent::ThinkingDelta(_) | EngineEvent::ToolInputDelta { .. } => true,
        EngineEvent::AgentEvent { event, .. } => streams(event),
        _ => false,
    }
}

impl Watched {
    fn locked(&self) -> MutexGuard<'_, HashMap<String, Watching>> {
        self.runs.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub fn see(&self, session: &str, event: &EngineEvent, load: impl FnOnce() -> Shown) -> bool {
        let mut runs = self.locked();
        if !runs.contains_key(session) {
            drop(runs);
            let shown = load();
            runs = self.locked();
            runs.entry(session.to_owned()).or_insert(Watching { shown, told: None, ended: None });
        }
        let Some(watching) = runs.get_mut(session) else { return false };
        watching.shown.transcript.apply(event);
        watching.shown.stamp(now_ms());
        if event.is_terminal() {
            watching.ended = Some(Instant::now());
        }
        let due = !streams(event) || watching.told.is_none_or(|told| told.elapsed() >= NOTIFY_GAP);
        if due {
            watching.told = Some(Instant::now());
        }
        due
    }

    pub fn read(&self, session: &str) -> Option<(HistoryStamp, Arc<Shown>)> {
        self.read.lock().unwrap_or_else(PoisonError::into_inner).get(session).cloned()
    }

    pub fn keep_read(&self, session: &str, stamp: HistoryStamp, shown: Arc<Shown>) {
        let mut read = self.read.lock().unwrap_or_else(PoisonError::into_inner);
        if read.len() >= READ_AT_MOST && !read.contains_key(session) {
            read.clear();
        }
        read.insert(session.to_owned(), (stamp, shown));
    }

    pub fn with<R>(&self, session: &str, read: impl FnOnce(&Shown) -> R) -> Option<R> {
        let mut runs = self.locked();
        if runs.get(session).and_then(|watching| watching.ended).is_some_and(|ended| ended.elapsed() > KEPT_AFTER_END) {
            runs.remove(session);
        }
        runs.get(session).map(|watching| read(&watching.shown))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_running_session_is_seen_live_and_streaming_is_told_at_most_every_gap() {
        let watched = Watched::default();
        assert!(watched.see("s", &EngineEvent::ThinkingDelta("a".into()), Shown::default));
        assert!(!watched.see("s", &EngineEvent::ThinkingDelta("b".into()), Shown::default));
        watched.see("s", &EngineEvent::TextDelta("Oi".into()), Shown::default);
        let entries = watched.with("s", |shown| shown.transcript.entries.len());
        assert_eq!(entries, Some(2));
        assert!(watched.see("s", &EngineEvent::Notice("n".into()), Shown::default), "a whole event is told at once");
        assert_eq!(watched.with("other", |_| ()), None);
    }
}
