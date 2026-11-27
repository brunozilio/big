use std::path::Path;

use agent_engine::sessions::{Entry, FileSessions, Sessions};
use domain::trace::Record;
use domain::{EngineEvent, SessionId};

use super::*;

fn kept(store: &FileSessions, dir: &Path, id: &SessionId, entries: Vec<Entry>) {
    for entry in entries {
        store.record(dir, id, &entry).expect("record");
    }
}

#[test]
fn a_sessions_history_is_what_was_asked_and_what_happened_in_order() {
    let dir = tempfile::tempdir().expect("dir");
    let store = FileSessions::new(dir.path().join("sessions"));
    let id = SessionId("s1".into());
    kept(
        &store,
        dir.path(),
        &id,
        vec![
            Entry::Prompted { at_ms: 1, prompt: "say hi".into() },
            Entry::Opened { at_ms: 1, run: domain::trace::RunInfo::default() },
            Entry::Event { at_ms: 2, event: EngineEvent::TextDelta("Hi.".into()) },
            Entry::Called { at_ms: 3, agent: None, call: domain::trace::ModelCall::default() },
            Entry::Event { at_ms: 4, event: EngineEvent::Completed { duration_ms: 3, cost_usd: None } },
        ],
    );

    assert_eq!(
        kept_history(&store, dir.path(), &id).expect("history"),
        [
            Record::Prompted { at_ms: 1, prompt: "say hi".into() },
            Record::Event { at_ms: 2, event: EngineEvent::TextDelta("Hi.".into()) },
            Record::Event { at_ms: 4, event: EngineEvent::Completed { duration_ms: 3, cost_usd: None } },
        ]
    );
}

#[test]
fn a_history_that_did_not_change_since_it_was_last_read_is_not_sent_again() {
    let dir = tempfile::tempdir().expect("dir");
    let store = FileSessions::new(dir.path().join("sessions"));
    let id = SessionId("s1".into());
    assert_eq!(kept_since(&store, dir.path(), &id, None).expect("nothing yet"), (None, Some(Vec::new())));
    kept(&store, dir.path(), &id, vec![Entry::Prompted { at_ms: 1, prompt: "one".into() }]);

    let (stamp, records) = kept_since(&store, dir.path(), &id, None).expect("first");
    assert_eq!(records, Some(vec![Record::Prompted { at_ms: 1, prompt: "one".into() }]));
    assert_eq!(kept_since(&store, dir.path(), &id, stamp).expect("again"), (stamp, None));

    kept(&store, dir.path(), &id, vec![Entry::Prompted { at_ms: 2, prompt: "two".into() }]);
    let (grown, records) = kept_since(&store, dir.path(), &id, stamp).expect("grown");
    assert_ne!(grown, stamp);
    assert_eq!(records.map(|records| records.len()), Some(2));
}

#[test]
fn another_folder_has_no_history_of_it() {
    let dir = tempfile::tempdir().expect("dir");
    let store = FileSessions::new(dir.path().join("sessions"));
    let id = SessionId("s1".into());
    kept(&store, dir.path(), &id, vec![Entry::Prompted { at_ms: 1, prompt: "mine".into() }]);

    assert_eq!(kept_history(&store, Path::new("/elsewhere"), &id).expect("history"), []);
}

#[test]
fn the_listing_is_the_catalog_and_an_unknown_session_has_no_history() {
    let services = crate::features::context::testing::services(None);
    let id = SessionId("s1".into());
    services.catalog.update(&mut |catalog| catalog.started(&id, Path::new("/repo"), 1));

    let listed = listed(&services);

    assert_eq!(listed.iter().map(|row| &row.session).collect::<Vec<_>>(), [&id]);
    assert_eq!(history(&services, Path::new("/repo"), &id).expect("history"), []);
}
