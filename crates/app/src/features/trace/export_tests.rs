use std::collections::BTreeMap;
use std::sync::atomic::{AtomicUsize, Ordering};

use agent_engine::model::http::NetHttp;
use domain::trace::runs::{Span, SpanKind};
use serde_json::Value;

use super::*;

struct Collector {
    url: String,
    received: crossbeam_channel::Receiver<(Value, Vec<(String, String)>)>,
}

fn collector(answer: u16) -> Collector {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("collector");
    listener.set_nonblocking(true).expect("non-blocking");
    let url = format!("http://{}", listener.local_addr().expect("address"));
    let (sent, received) = crossbeam_channel::unbounded();
    let answer = axum::http::StatusCode::from_u16(answer).expect("status");
    let router =
        axum::Router::new().fallback(move |uri: axum::http::Uri, headers: axum::http::HeaderMap, body: String| {
            let sent = sent.clone();
            async move {
                let headers = headers
                    .iter()
                    .map(|(name, value)| (name.as_str().to_owned(), value.to_str().unwrap_or_default().to_owned()))
                    .collect();
                let _ = sent.send((serde_json::from_str(&body).unwrap_or(Value::Null), headers));
                if uri.path() == "/v1/traces" { answer } else { axum::http::StatusCode::NOT_FOUND }
            }
        });
    platform::runtime::shared().expect("runtime").spawn(async move {
        let listener = tokio::net::TcpListener::from_std(listener).expect("listener");
        let _ = axum::serve(listener, router).await;
    });
    Collector { url, received }
}

fn run(session: &str, index: usize, status: Status) -> Run {
    let root = Span {
        id: 1,
        parent: 0,
        kind: SpanKind::Run,
        name: "run".into(),
        start_ms: 0,
        end_ms: 10,
        failed: false,
        input: String::new(),
        output: String::new(),
        thinking: String::new(),
        usage: None,
        context: None,
        stop: None,
        attrs: Vec::new(),
        flags: Vec::new(),
        request: None,
        reply: None,
        system_id: None,
        tools_id: None,
    };
    Run {
        session: session.into(),
        index,
        prompt: String::new(),
        started_at_ms: 1_000,
        duration_ms: 10,
        waiting_ms: 0,
        model: "m".into(),
        status,
        cost_usd: 0.0,
        tokens: 0,
        input: 0,
        cache_read: 0,
        waste: 0.0,
        signals: Vec::new(),
        spans: vec![root],
        messages: Vec::new(),
        systems: BTreeMap::new(),
        tools: BTreeMap::new(),
        trace: None,
        agent: None,
        attribution: Default::default(),
    }
}

fn runs_in(sent: &Value) -> Vec<u64> {
    let spans = sent["resourceSpans"][0]["scopeSpans"][0]["spans"].as_array().cloned().unwrap_or_default();
    spans
        .iter()
        .filter_map(|span| {
            let attributes = span["attributes"].as_array()?;
            let index = attributes.iter().find(|one| one["key"] == format!("{}.run.index", brand::COMMAND))?;
            index["value"]["intValue"].as_str()?.parse().ok()
        })
        .collect()
}

fn otlp(url: &str) -> Otlp {
    Otlp::from(url, "").expect("configured")
}

const WAIT: Duration = Duration::from_secs(20);

#[test]
fn the_endpoint_and_headers_come_from_the_config_and_nothing_is_exported_without_them() {
    assert_eq!(Otlp::from("", "x-key=abc"), None);
    assert_eq!(Otlp::from("localhost:4318", ""), None, "a URL with its scheme");
    let configured = Otlp::from("http://localhost:4318/", "x-key=abc, x-team = core,bad").expect("configured");
    assert_eq!(configured.url, "http://localhost:4318/v1/traces");
    assert_eq!(
        configured.headers,
        [
            ("content-type".to_owned(), "application/json".to_owned()),
            ("x-key".to_owned(), "abc".to_owned()),
            ("x-team".to_owned(), "core".to_owned())
        ]
    );
    let full = Otlp::from("https://otel.example/v1/traces", "").expect("full");
    let shipped = Otlp::from(brand::traces::OTLP_ENDPOINT, brand::traces::OTLP_HEADERS);
    assert_eq!(shipped.is_some(), !brand::traces::OTLP_ENDPOINT.trim().is_empty());
    assert_eq!(full.url, "https://otel.example/v1/traces");
}

#[test]
fn finished_runs_reach_the_collector_once_each_and_a_run_still_going_waits() {
    let collector = collector(200);
    let mut otlp = otlp(&collector.url);
    otlp.headers.push(("x-key".into(), "abc".into()));
    let finished = Arc::new(AtomicUsize::new(1));
    let seen = Arc::clone(&finished);
    let traced: Traced = Arc::new(move |_cwd: &Path, session: &SessionId| {
        let done = seen.load(Ordering::SeqCst);
        let mut runs: Vec<Run> = (0..=done).map(|index| run(&session.0, index, Status::Ok)).collect();
        runs.push(run(&session.0, done + 1, Status::Running));
        Ok(runs)
    });
    let exporter = Exporter::start(otlp, traced, Arc::new(NetHttp)).expect("exporter");
    let session = SessionId("s-1".into());

    exporter.finished(Path::new("/repo"), &session);
    let (sent, headers) = collector.received.recv_timeout(WAIT).expect("first export");
    assert_eq!(runs_in(&sent), [1], "a session seen for the first time sends its latest finished run only");
    assert!(headers.contains(&("x-key".into(), "abc".into())), "{headers:?}");
    assert!(headers.iter().any(|(name, value)| name == "content-type" && value == "application/json"));

    finished.store(3, Ordering::SeqCst);
    exporter.finished(Path::new("/repo"), &session);
    exporter.finished(Path::new("/repo"), &session);
    let (sent, _) = collector.received.recv_timeout(WAIT).expect("second export");
    assert_eq!(runs_in(&sent), [2, 3], "the runs finished since, batched into one request, never twice");
    assert!(collector.received.recv_timeout(Duration::from_millis(2_500)).is_err(), "nothing new, nothing sent");
}

#[test]
fn a_collector_that_is_down_or_refuses_never_holds_up_a_run_and_is_logged_once() {
    let refusing = collector(500);
    let traced: Traced = Arc::new(|_: &Path, session: &SessionId| Ok(vec![run(&session.0, 0, Status::Ok)]));
    let exporter = Exporter::start(otlp(&refusing.url), Arc::clone(&traced), Arc::new(NetHttp)).expect("exporter");
    let started = Instant::now();
    for at in 0..QUEUED_AT_MOST + 10 {
        exporter.finished(Path::new("/repo"), &SessionId(format!("s-{at}")));
    }
    assert!(started.elapsed() < Duration::from_secs(1), "queuing took {:?}", started.elapsed());
    assert!(refusing.received.recv_timeout(WAIT).is_ok(), "it was still tried");
    let deadline = Instant::now() + WAIT;
    while !exporter.said.once.load(std::sync::atomic::Ordering::Relaxed) && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(exporter.said.once.load(std::sync::atomic::Ordering::Relaxed), "the refusal or the full queue was said");

    let closed = std::net::TcpListener::bind("127.0.0.1:0").expect("port");
    let url = format!("http://{}", closed.local_addr().expect("address"));
    drop(closed);
    let down = Exporter::start(otlp(&url), traced, Arc::new(NetHttp)).expect("exporter");
    down.finished(Path::new("/repo"), &SessionId("s".into()));
    let said = Said::default();
    assert!(said.failed("first"), "the first failure is logged");
    assert!(!said.failed("second"), "later ones are not");
}

#[test]
fn a_watched_run_is_exported_under_the_session_it_started_or_resumed() {
    let collector = collector(200);
    let traced: Traced = Arc::new(|_: &Path, session: &SessionId| Ok(vec![run(&session.0, 0, Status::Ok)]));
    let exporter = Exporter::start(otlp(&collector.url), traced, Arc::new(NetHttp)).expect("exporter");

    let fresh = exporter.watch(Path::new("/repo"), None);
    fresh.observe(&EngineEvent::TextDelta("hi".into()));
    fresh.observe(&EngineEvent::SessionStarted(SessionId("new".into())));
    fresh.observe(&EngineEvent::Completed { duration_ms: 1, cost_usd: None });
    let (sent, _) = collector.received.recv_timeout(WAIT).expect("export");
    let session = &sent["resourceSpans"][0]["scopeSpans"][0]["spans"][0]["attributes"];
    assert!(session.to_string().contains("\"new\""), "{session}");

    let unknown = exporter.watch(Path::new("/repo"), None);
    unknown.observe(&EngineEvent::Failed("no session".into()));
    let resumed = exporter.watch(Path::new("/repo"), Some(&SessionId("old".into())));
    resumed.observe(&EngineEvent::Cancelled);
    let (sent, _) = collector.received.recv_timeout(WAIT).expect("export");
    assert!(sent.to_string().contains("\"old\"") && !sent.to_string().contains("no session"));
}

fn project_secrets() -> Secrets {
    Arc::new(agent_engine::tools::env_secrets)
}

#[test]
fn capture_starts_off_and_a_limit_that_is_not_a_positive_number_keeps_it_off() {
    assert_eq!(brand::traces::CAPTURE_CONTENT, "false", "capture starts off");
    let redactor = || Arc::new(domain::redact::Redactor::default());
    let limit = brand::traces::CAPTURE_LIMIT;
    let on = Keys { content: true, ..Keys::default() };
    assert!(Content::from(Keys::configured(), limit, redactor, project_secrets()).is_none());
    assert!(Content::from(on, limit, redactor, project_secrets()).is_some(), "the shipped limit is valid");
    for broken in ["", "0", "-3", "many"] {
        assert!(Content::from(on, broken, redactor, project_secrets()).is_none(), "{broken:?}");
    }
}

#[test]
fn turned_on_the_content_leaves_without_personal_data_or_the_project_s_secrets() {
    let project = tempfile::tempdir().expect("project");
    std::fs::write(project.path().join(".env"), "API_TOKEN=projectsecret42\n").expect("env");
    let collector = collector(200);
    let traced: Traced = Arc::new(|_: &Path, session: &SessionId| {
        let mut said = run(&session.0, 0, Status::Ok);
        said.spans[0].input = "call me at ana@example.com, the key is projectsecret42".into();
        Ok(vec![said])
    });
    let exporter = Exporter::start(otlp(&collector.url), traced, Arc::new(NetHttp)).expect("exporter");
    let personal = || agent_engine::tools::personal(&domain::redact::Redactor::default());
    let keys = Keys { usage: true, content: true, prompts: true };
    let content = Content::from(keys, "4096", personal, project_secrets());
    exporter.capturing(Policy { content, usage: true, revision: "rev-1".into() });
    exporter.finished(project.path(), &SessionId("s".into()));
    let (sent, _) = collector.received.recv_timeout(WAIT).expect("export");
    let sent = sent.to_string();
    assert!(sent.contains("call me at [redacted], the key is [redacted]"), "{sent}");
    assert!(!sent.contains("ana@example.com") && !sent.contains("projectsecret42"));
    assert!(sent.contains("policy_revision") && sent.contains("rev-1"), "{sent}");
}
