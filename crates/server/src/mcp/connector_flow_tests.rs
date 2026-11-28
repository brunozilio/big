use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::Duration;

use connectors::descriptor::object;
use connectors::{ActionError, ActionSpec, Idempotency, Labelled};
use domain::label::{Levels, Mode, Read, Trust};
use domain::operation::{Kind, Profile, Reversible, Risk};
use domain::permission::Surface;
use serde_json::json;

use super::Guarded;
use super::session_reads::{close, open};
use app::policy_gate::Asked;

const ASKED: Asked<'static> = Asked { surface: Surface::Mcp, stage: "tools/call", owner: "local", operation: None };

fn levels(mode: Mode) -> Levels {
    Levels { mode, levels: vec!["public".into(), "internal".into(), "confidential".into()] }
}

fn reading() -> ActionSpec {
    let profile = Profile::new(vec![Kind::Read], Risk::Low, Reversible::Yes, true, true);
    ActionSpec::read("notes.get", object(&[], &[]), json!({}), profile)
}

fn sending() -> ActionSpec {
    let profile = Profile::new(vec![Kind::Write], Risk::Low, Reversible::Yes, false, false);
    ActionSpec {
        max_input_level: Some("internal".into()),
        ..ActionSpec::write("notes.send", object(&[], &[]), json!({}), Idempotency::Native, profile)
    }
}

fn answer(label: Option<&str>) -> Result<Labelled, ActionError> {
    let read = Read::new("Notes notes.get", Trust::External, label.map(str::to_owned));
    Ok(Labelled { value: json!({ "body": "a <b>secret</b>" }), read })
}

fn sent() -> Result<Labelled, ActionError> {
    Ok(Labelled { value: json!("sent"), read: Read::new("Notes notes.send", Trust::System, None) })
}

fn guarded<'a>(session: Option<&'a str>, spec: &'a ActionSpec, levels: &'a Levels) -> Guarded<'a> {
    Guarded { asked: ASKED, session, tool: "notes_send", spec, levels }
}

#[test]
fn an_external_result_is_wrapped_and_a_confidential_read_stops_the_send_only_when_enforced() {
    for mode in [Mode::Shadow, Mode::Enforce] {
        let (lattice, session) = (levels(mode), format!("s-{}", mode.word()));
        open(&session);
        let read = guarded(Some(&session), &reading(), &lattice).run(|| answer(Some("confidential")));
        let shown = read.expect("reading is never refused");
        assert!(shown.starts_with("<external-data source=\"Notes notes.get\""), "{shown}");
        assert!(shown.contains("a &lt;b&gt;secret&lt;/b&gt;"), "{shown}");
        let send = guarded(Some(&session), &sending(), &lattice).run(sent);
        match mode {
            Mode::Shadow => assert_eq!(send.as_deref(), Ok("sent")),
            Mode::Enforce => {
                let refused = send.expect_err("past the ceiling");
                assert!(refused.starts_with("unavailable: ") && refused.contains("internal ceiling"), "{refused}");
            }
        }
        close(&session);
    }
}

#[test]
fn a_closed_or_unknown_session_counts_as_the_most_restricted_only_when_enforced() {
    open("s-closed");
    close("s-closed");
    for session in ["s-closed", "s-unknown"] {
        let refused = guarded(Some(session), &sending(), &levels(Mode::Enforce)).run(sent);
        assert!(refused.is_err_and(|told| told.contains("confidential")), "{session}");
        assert_eq!(guarded(Some(session), &sending(), &levels(Mode::Shadow)).run(sent).as_deref(), Ok("sent"));
    }
}

#[test]
fn a_call_without_a_session_is_refused_only_when_enforced() {
    let refused = guarded(None, &reading(), &levels(Mode::Enforce)).run(|| answer(None));
    assert!(refused.is_err_and(|told| told.contains("inside an MCP session")));
    assert!(guarded(None, &reading(), &levels(Mode::Shadow)).run(|| answer(None)).is_ok());
    assert!(guarded(None, &sending(), &levels(Mode::Shadow)).run(sent).is_ok());
}

#[test]
fn two_calls_in_one_enforced_session_see_each_other() {
    open("s-together");
    let lattice = levels(Mode::Enforce);
    let (inside, told) = mpsc::channel();
    let sent_anyway = AtomicBool::new(false);
    let reading = reading();
    std::thread::scope(|scope| {
        let first = scope.spawn(|| {
            guarded(Some("s-together"), &reading, &lattice).run(|| {
                let _ = inside.send(());
                std::thread::sleep(Duration::from_millis(200));
                answer(Some("confidential"))
            })
        });
        told.recv().expect("the read started");
        let second = guarded(Some("s-together"), &sending(), &lattice).run(|| {
            sent_anyway.store(true, Ordering::SeqCst);
            sent()
        });
        assert!(first.join().expect("first").is_ok());
        assert!(second.is_err_and(|told| told.contains("internal ceiling")));
    });
    assert!(!sent_anyway.load(Ordering::SeqCst), "the send waited for the read and never ran");
    close("s-together");
}
