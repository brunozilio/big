#![allow(clippy::expect_used, reason = "a failed setup step stops the test")]

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use agent_engine::fake::FakeEngine;
use app::{Client, EngineEvent, LocalServices, Model, SettingsStore};
use crossbeam_channel::{Receiver, Sender};
use domain::SessionId;

struct Stuck {
    entered: AtomicBool,
    released: Receiver<()>,
}

impl SettingsStore for Stuck {
    fn load(&self) -> Option<serde_json::Value> {
        self.entered.store(true, Ordering::SeqCst);
        let _ = self.released.recv();
        None
    }

    fn save(&self, _settings: &serde_json::Value) -> Result<(), app::SettingsError> {
        Ok(())
    }
}

fn model() -> Model {
    Model::new("scripted", "Scripted")
}

fn with_a_stuck_query() -> (Client, Arc<Stuck>, Sender<()>, FakeEngine) {
    let script = vec![
        EngineEvent::SessionStarted(SessionId("s".into())),
        EngineEvent::Completed { duration_ms: 1, cost_usd: None },
    ];
    let engine = FakeEngine::new(vec![model()], script);
    let (release, released) = crossbeam_channel::unbounded();
    let stuck = Arc::new(Stuck { entered: AtomicBool::new(false), released });
    let mut services = LocalServices::with_engine(Arc::new(engine.clone()));
    services.settings = Arc::clone(&stuck) as Arc<dyn SettingsStore>;
    (Client::builder().services(services).build().expect(brand::COMMAND), stuck, release, engine)
}

#[test]
fn a_run_or_a_side_question_never_waits_for_a_slow_query() {
    let (client, stuck, release, engine) = with_a_stuck_query();
    let asking = client.clone();
    let listing = std::thread::spawn(move || asking.sessions());
    let deadline = Instant::now() + Duration::from_secs(5);
    while !stuck.entered.load(Ordering::SeqCst) {
        assert!(Instant::now() < deadline, "the backend never took the query it is stuck on");
        std::thread::sleep(Duration::from_millis(2));
    }
    let session = client.session("/repo", model()).resume(SessionId("s".into()));

    let asked = Instant::now();
    let prompt = session.send("hi").expect("enqueued");
    let side = session.side("what is this?").expect("enqueued");
    for run in [prompt, side] {
        let ended = run.last();
        assert!(matches!(ended, Some(EngineEvent::Completed { .. })), "{ended:?}");
    }
    assert!(asked.elapsed() < Duration::from_secs(5), "{:?}", asked.elapsed());
    assert_eq!(engine.requests().len(), 2, "both ran while the query was still stuck");

    release.send(()).expect("released");
    assert!(listing.join().expect("join").is_ok());
}
