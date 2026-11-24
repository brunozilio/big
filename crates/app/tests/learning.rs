#![allow(clippy::expect_used, reason = "a failed setup step stops the test")]

use std::sync::Arc;
use std::time::{Duration, Instant};

use agent_engine::fake::FakeEngine;
use app::{Client, EngineEvent, Model};
use app::{InMemorySecretStore, LocalServices};
use domain::{PromptRequest, SessionCommand, SessionId};

fn model() -> Model {
    Model::new("scripted", "Scripted")
}

fn text(run: app::Run) -> String {
    run.filter_map(|event| match event {
        EngineEvent::TextDelta(text) => Some(text),
        _ => None,
    })
    .collect()
}

fn learned(engine: &FakeEngine) -> Option<PromptRequest> {
    engine.requests().into_iter().find(|request| request.session.command == Some(SessionCommand::Learn))
}

#[test]
fn a_run_the_user_corrected_enough_proposes_a_lesson_on_its_own_session() {
    let data = tempfile::tempdir().expect("data");
    let engine = FakeEngine::new(
        vec![model()],
        vec![
            EngineEvent::SessionStarted(SessionId("s-1".into())),
            EngineEvent::Steered("no, use pnpm".into()),
            EngineEvent::Steered("again: pnpm, not npm".into()),
            EngineEvent::Completed { duration_ms: 1, cost_usd: None },
        ],
    );
    let mut services = LocalServices::with_engine(Arc::new(engine.clone()));
    services.providers = services.providers.with_secrets(Arc::new(InMemorySecretStore::default()));
    let client = Client::builder().services(services.kept_in(data.path())).build().expect(brand::COMMAND);
    let session = client.session(data.path(), model());

    text(session.send("install it").expect("run"));

    let until = Instant::now() + Duration::from_secs(5);
    while learned(&engine).is_none() && Instant::now() < until {
        std::thread::sleep(Duration::from_millis(10));
    }
    let lesson = learned(&engine).expect("a lesson was asked for");
    assert!(lesson.prompt.text.contains("no, use pnpm") && lesson.prompt.text.contains("again: pnpm, not npm"));
    assert_eq!(lesson.session.resume, Some(SessionId("s-1".into())), "on the corrected session");
    let lessons = engine.requests().iter().filter(|r| r.session.command == Some(SessionCommand::Learn)).count();
    assert_eq!(lessons, 1, "a lesson run does not ask for another lesson");
}
