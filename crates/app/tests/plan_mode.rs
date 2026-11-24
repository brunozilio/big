#![allow(clippy::expect_used, reason = "a failed setup step stops the test")]

use std::sync::Arc;

use agent_engine::fake::FakeEngine;
use app::{Client, EngineEvent, Model};
use app::{InMemorySecretStore, LocalServices};
use domain::{AgentAsk, AskId, AskKind, PermissionMode, SessionId};

fn model() -> Model {
    Model::new("scripted", "Scripted")
}

fn client(engine: &FakeEngine, data: &std::path::Path) -> Client {
    let mut services = LocalServices::with_engine(Arc::new(engine.clone()));
    services.providers = services.providers.with_secrets(Arc::new(InMemorySecretStore::default()));
    Client::builder().services(services.kept_in(data)).build().expect(brand::COMMAND)
}

const PLAN: &str = "## Approach\nEdit it.\n\n## Verification\ncargo test passes";

fn run(middle: Vec<EngineEvent>) -> Vec<EngineEvent> {
    let mut events = vec![EngineEvent::SessionStarted(SessionId("s-1".into()))];
    events.extend(middle);
    events.push(EngineEvent::TextDelta("Implemented.".into()));
    events.push(EngineEvent::Completed { duration_ms: 1, cost_usd: None });
    events
}

fn presented() -> EngineEvent {
    EngineEvent::Asked(AgentAsk { id: AskId("p".into()), kind: AskKind::Plan { plan: PLAN.into() } })
}

fn modes(engine: &FakeEngine) -> Vec<PermissionMode> {
    engine.requests().into_iter().map(|request| request.policy.permission).collect()
}

#[test]
fn an_approved_plan_leaves_the_session_in_the_mode_chosen() {
    let data = tempfile::tempdir().expect("data");
    let engine =
        FakeEngine::new(vec![model()], run(vec![presented(), EngineEvent::ModeChanged(PermissionMode::AcceptEdits)]));
    let client = client(&engine, data.path());
    let mut session = client.session(data.path(), model());
    session.set_permission(PermissionMode::Plan);
    let events: Vec<EngineEvent> = session.send("plan it").expect("run").collect();
    assert!(matches!(events.last(), Some(EngineEvent::Completed { .. })));

    let _: Vec<EngineEvent> = session.send("more").expect("run").collect();
    assert_eq!(modes(&engine), [PermissionMode::Plan, PermissionMode::AcceptEdits]);
}

#[test]
fn entering_plan_mode_keeps_the_session_planning() {
    let data = tempfile::tempdir().expect("data");
    let engine = FakeEngine::new(vec![model()], run(vec![EngineEvent::ModeChanged(PermissionMode::Plan)]));
    let client = client(&engine, data.path());
    let session = client.session(data.path(), model());
    let _: Vec<EngineEvent> = session.send("big change").expect("run").collect();
    let _: Vec<EngineEvent> = session.send("go on").expect("run").collect();
    assert_eq!(modes(&engine)[1], PermissionMode::Plan);
}
