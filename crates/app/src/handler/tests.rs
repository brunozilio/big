use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use crate::api::{ClientCommand, Envelope, ProviderEvent, Reply, RequestId, ServerEvent};
use domain::provider::ModelProvider;

use super::*;

type Sent = crossbeam_channel::Receiver<ServerEvent>;

fn handler(services: LocalServices) -> (CommandHandler, Sent) {
    let (events, sent) = crossbeam_channel::unbounded();
    (CommandHandler::new(services, Outbox::new(events, Arc::new(|| {}))), sent)
}

fn answer_to(sent: &Sent, id: u64) -> Reply {
    match sent.recv_timeout(Duration::from_secs(5)) {
        Ok(ServerEvent::Reply { id: answered, reply }) if answered == RequestId(id) => reply,
        other => panic!("expected an answer to request {id}, got {other:?}"),
    }
}

#[derive(Default)]
struct HoldsThePort {
    held: AtomicBool,
}

impl crate::providers::SignIn for HoldsThePort {
    fn sign_in(
        &self,
        provider: ModelProvider,
        _back: Option<&str>,
        _fields: &serde_json::Value,
        stop: &Arc<AtomicBool>,
        open: &mut dyn FnMut(String, Option<String>),
    ) -> Result<llm_providers::Config, llm_providers::ProviderError> {
        self.held.store(true, Ordering::SeqCst);
        open(format!("https://auth.test/{}", provider.id()), None);
        while !stop.load(Ordering::SeqCst) {
            std::thread::sleep(Duration::from_millis(5));
        }
        self.held.store(false, Ordering::SeqCst);
        Err(llm_providers::ProviderError::Invalid("stopped".into()))
    }
}

#[test]
fn cancelling_a_sign_in_frees_the_port() {
    llm_providers::testing::install();
    let browser = ModelProvider::new("browser-sub");
    let waiting = Arc::new(HoldsThePort::default());
    let mut services = crate::features::context::testing::services(None);
    services.providers = services.providers.with_sign_in(waiting.clone());
    let (handler, sent) = handler(services);
    handler.handle(Envelope::local(RequestId(1), ClientCommand::SignInProvider { provider: browser, back: None, fields: serde_json::Value::Null }));
    assert!(matches!(answer_to(&sent, 1), Reply::Providers(ProviderEvent::AuthorizeAt { .. })));
    assert!(waiting.held.load(Ordering::SeqCst), "the sign-in is waiting on its port");

    handler.handle(Envelope::local(RequestId(2), ClientCommand::CancelRequest { request: RequestId(1) }));

    assert!(!waiting.held.load(Ordering::SeqCst), "the port is free by the time the cancel is acknowledged");
    match answer_to(&sent, 1) {
        Reply::Providers(ProviderEvent::Failed { provider, message }) => {
            assert_eq!(provider, browser);
            assert!(message.contains("cancelled"), "{message}");
        }
        other => panic!("expected the sign-in to end, got {other:?}"),
    }
    assert_eq!(answer_to(&sent, 2), Reply::Done);
    let more = sent.recv_timeout(Duration::from_millis(100));
    assert!(more.is_err(), "each request ends once: {more:?}");
}

#[test]
fn cancelling_a_request_that_already_ended_changes_nothing() {
    let (handler, sent) = handler(crate::features::context::testing::services(None));
    handler.handle(Envelope::local(RequestId(1), ClientCommand::ListSessions));
    handler.handle(Envelope::local(RequestId(2), ClientCommand::CancelRequest { request: RequestId(1) }));
    assert_eq!(answer_to(&sent, 1), Reply::Sessions(Vec::new()));
    assert_eq!(answer_to(&sent, 2), Reply::Done);
}
