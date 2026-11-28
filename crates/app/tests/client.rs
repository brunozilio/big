#![allow(clippy::expect_used, reason = "a failed setup step stops the test")]

use std::sync::Arc;

use agent_engine::fake::FakeEngine;
use app::{Client, Decision, EngineEvent, Model};
use app::{InMemorySecretStore, LocalServices};
use domain::{AgentAsk, AskId, AskKind, ChoreAnswer, SessionCommand, SessionId};

fn model() -> Model {
    Model::new("scripted", "Scripted")
}

fn script(extra: Vec<EngineEvent>) -> Vec<EngineEvent> {
    let mut events = vec![EngineEvent::SessionStarted(SessionId("s-1".into()))];
    events.extend(extra);
    events.push(EngineEvent::TextDelta("Hello ".into()));
    events.push(EngineEvent::TextDelta("there".into()));
    events.push(EngineEvent::Completed { duration_ms: 1, cost_usd: None });
    events
}

fn services(engine: &FakeEngine) -> LocalServices {
    let mut services = LocalServices::with_engine(Arc::new(engine.clone()));
    let secrets = Arc::new(InMemorySecretStore::default());
    services.providers = services.providers.with_secrets(secrets);
    services
}

fn build(engine: &FakeEngine, data: &std::path::Path) -> Client {
    Client::builder().services(services(engine).kept_in(data)).build().expect(brand::COMMAND)
}

fn text(run: app::Run) -> String {
    run.filter_map(|event| match event {
        EngineEvent::TextDelta(text) => Some(text),
        _ => None,
    })
    .collect()
}

#[test]
fn a_session_streams_a_run_and_the_next_prompt_resumes_it() {
    let data = tempfile::tempdir().expect("data");
    let engine = FakeEngine::new(vec![model()], script(Vec::new()));
    let client = build(&engine, data.path());
    assert_eq!(client.models().expect("models").iter().map(|m| m.id.as_str()).collect::<Vec<_>>(), ["scripted"]);

    let session = client.session(data.path(), model());
    assert_eq!(text(session.send("hi").expect("run")), "Hello there");
    assert_eq!(session.id(), Some(SessionId("s-1".into())));
    let events: Vec<EngineEvent> = session.send("again").expect("run").collect();
    assert!(matches!(events.last(), Some(EngineEvent::Completed { .. })));

    let requests = engine.requests();
    assert_eq!(requests[0].session.resume, None);
    assert_eq!(requests[1].session.resume, Some(SessionId("s-1".into())), "the session continues");
    assert!(requests[0].host.memory.is_some(), "memory is on");
}

#[test]
fn a_question_is_answered_through_the_run() {
    let data = tempfile::tempdir().expect("data");
    let ask = AgentAsk { id: AskId("a-1".into()), kind: AskKind::permission("Bash", "ls", false) };
    let engine = FakeEngine::new(vec![model()], script(vec![EngineEvent::Asked(ask)]))
        .with_delay(std::time::Duration::from_millis(40));
    let client = build(&engine, data.path());
    let mut run = client.session(data.path(), model()).send("list files").expect("run");
    while let Some(event) = run.next_event() {
        if let EngineEvent::Asked(ask) = event {
            run.answer(ask.id, Decision::Allow).expect("answer");
        }
    }
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while engine.answers().is_empty() && std::time::Instant::now() < deadline {
        std::thread::yield_now();
    }
    assert_eq!(engine.answers(), [(AskId("a-1".into()), Decision::Allow)]);
}

#[test]
fn providers_are_listed() {
    let data = tempfile::tempdir().expect("data");
    agent_engine::providers::install_testing();
    let engine = FakeEngine::new(vec![model()], Vec::new());
    let client = build(&engine, data.path());
    let listed = client.providers().expect("providers");
    assert!(!listed.is_empty() && listed.iter().all(|p| p.account.is_none()));
}

#[test]
fn a_session_s_effort_and_mode_reach_the_run_and_a_control_cancels_it_from_elsewhere() {
    let data = tempfile::tempdir().expect("data");
    let engine = FakeEngine::new(vec![model()], script(Vec::new())).with_delay(std::time::Duration::from_millis(100));
    let client = build(&engine, data.path());
    let mut session = client.session(data.path(), model()).effort(app::Effort::Max);
    session.set_permission(app::PermissionMode::Plan);
    let run = session.send("think hard").expect("run");
    let control = run.control();
    std::thread::spawn(move || control.cancel().expect("cancel"));
    let events: Vec<EngineEvent> = run.collect();
    assert_eq!(events.last(), Some(&EngineEvent::Cancelled));
    let request = engine.requests().remove(0);
    assert_eq!((request.session.effort, request.policy.permission), (app::Effort::Max, app::PermissionMode::Plan));
}

#[test]
fn a_side_question_is_answered_from_the_conversation_without_continuing_it() {
    let data = tempfile::tempdir().expect("data");
    let answer = EngineEvent::Chore(ChoreAnswer::Side("It edits main.rs.".into()));
    let engine = FakeEngine::new(vec![model()], script(vec![answer]));
    let client = build(&engine, data.path());
    let session = client.session(data.path(), model());
    assert!(matches!(session.side("too early?"), Err(app::Error::NothingToAsk)));

    text(session.send("fix the bug").expect("run"));
    assert_eq!(session.side("which file?").expect("run").side_answer().expect("answer"), "It edits main.rs.");

    let side = engine.requests().remove(1);
    assert_eq!(side.session.command, Some(SessionCommand::Side));
    assert_eq!(side.prompt.text, "which file?");
    assert_eq!(side.session.resume, Some(SessionId("s-1".into())), "asked about this conversation");
    assert!(side.host.memory.is_none(), "a side question recalls no memory");
}

#[test]
fn a_suggestion_needs_a_conversation_and_resumes_it() {
    let data = tempfile::tempdir().expect("data");
    let answer = EngineEvent::Chore(ChoreAnswer::Suggestion("run the tests".into()));
    let engine = FakeEngine::new(vec![model()], script(vec![answer]));
    let client = build(&engine, data.path());
    let session = client.session(data.path(), model());
    assert!(matches!(session.suggest(), Err(app::Error::NothingToAsk)));

    text(session.send("fix the bug").expect("run"));
    let suggested = session.suggest().expect("run").chore_answer().expect("answer");

    assert_eq!(suggested, Some(ChoreAnswer::Suggestion("run the tests".into())));
    let asked: Vec<_> = engine.requests().iter().map(|r| (r.session.command, r.session.resume.is_some())).collect();
    assert_eq!(asked, [(None, false), (Some(SessionCommand::Suggest), true)]);
}
