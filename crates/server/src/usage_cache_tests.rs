use std::cell::Cell;
use std::path::PathBuf;

use super::*;
use domain::SessionId;
use domain::billing::ModelUsage;

fn row(session: &str, updated: u64, status: SessionStatus) -> Listed {
    Listed {
        session: SessionId(session.to_owned()),
        cwd: PathBuf::from("/tmp"),
        title: None,
        created: 0,
        updated,
        status,
        unread: false,
    }
}

fn history() -> Vec<Record> {
    let usage = EngineEvent::Usage(ModelUsage { estimated_cost_usd: Some(0.5), ..ModelUsage::default() });
    vec![Record::Event { at_ms: 1, event: usage }, Record::Event { at_ms: 2, event: EngineEvent::Steered("ok".into()) }]
}

#[test]
fn a_settled_session_is_read_once_and_keeps_only_what_it_spent() {
    let mut cache = UsageCache::default();
    let reads = Cell::new(0);
    let read = || {
        reads.set(reads.get() + 1);
        Some(history())
    };
    let done = row("s", 10, SessionStatus::Done);
    assert_eq!(cache.spent(&done, read).len(), 1);
    assert_eq!(cache.spent(&done, read).len(), 1);
    assert_eq!(reads.get(), 1);
    cache.spent(&row("s", 11, SessionStatus::Done), read);
    assert_eq!(reads.get(), 2);
}

#[test]
fn a_running_session_is_read_every_time() {
    let mut cache = UsageCache::default();
    let reads = Cell::new(0);
    let read = || {
        reads.set(reads.get() + 1);
        Some(history())
    };
    let running = row("s", 10, SessionStatus::Running);
    cache.spent(&running, read);
    cache.spent(&running, read);
    assert_eq!(reads.get(), 2);
}

#[test]
fn an_unreadable_or_vanished_session_is_forgotten() {
    let mut cache = UsageCache::default();
    let done = row("s", 10, SessionStatus::Done);
    cache.spent(&done, || Some(history()));
    assert!(cache.spent(&done, || None).len() == 1);
    assert!(cache.spent(&row("s", 11, SessionStatus::Done), || None).is_empty());
    cache.spent(&done, || Some(history()));
    cache.keep_only(&[]);
    assert!(cache.spent(&done, || None).is_empty());
}

#[test]
fn a_sub_agent_s_announcement_is_kept_without_its_prompt() {
    let mut cache = UsageCache::default();
    let spawned = |prompt: &str| EngineEvent::AgentSpawned {
        index: 1,
        kind: "explore".into(),
        summary: "look".into(),
        prompt: prompt.into(),
    };
    let nested = EngineEvent::AgentEvent { index: 1, event: Box::new(spawned("inner")) };
    let read = || {
        Some(vec![Record::Event { at_ms: 1, event: spawned("a long task") }, Record::Event { at_ms: 2, event: nested }])
    };
    let kept = cache.spent(&row("s", 10, SessionStatus::Done), read);
    assert_eq!(kept, [(1, spawned(""))]);
}
