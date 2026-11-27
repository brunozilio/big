use std::sync::Arc;

use serde_json::{Value, json};

use super::{Conversation, body};
use crate::catalog::{Call, Page};
use crate::model::http::ScriptedHttp;
use crate::model::{BlockingTurn, CancellationToken, Delta, ModelError, TurnRequest};
use crate::transcript::{Block, Message, StopReason};
use crate::{Config, Provider, ProviderError};

struct Framed;

impl Provider for Framed {
    fn id(&self) -> &'static str {
        "framed"
    }
    fn name(&self) -> &'static str {
        "Framed"
    }
    fn vendor(&self) -> &'static str {
        ""
    }
    fn access(&self) -> domain::Access {
        domain::Access::default()
    }
    fn rank(&self) -> u8 {
        0
    }
    fn slot(&self) -> u128 {
        0
    }
    fn window(&self) -> u64 {
        0
    }
    fn wire(&self, _model: Option<&domain::ModelOption>) -> &'static str {
        super::NAME
    }
    fn base(&self, _config: &Config) -> String {
        String::new()
    }
    fn headers(&self, _config: &Config) -> Vec<(String, String)> {
        Vec::new()
    }
    fn path(&self, _model: Option<&domain::ModelOption>) -> &'static str {
        ""
    }
    fn shape(
        &self,
        _model: Option<&domain::ModelOption>,
        _body: &mut Value,
        _effort: &str,
        _intent: &Value,
    ) -> Result<(), ProviderError> {
        Ok(())
    }
    fn catalog(&self, _config: &Config, _after: &str) -> Call {
        Call { url: String::new(), headers: Vec::new(), body: None }
    }
    fn page(&self, _value: &Value) -> Result<Page, ProviderError> {
        Err(ProviderError::Invalid("no catalog".into()))
    }
    fn frame(&self, state: &Value, frame: &Value) -> Result<Value, ProviderError> {
        let seen = state.as_u64().unwrap_or_default() + 1;
        if let Some(text) = frame["t"].as_str() {
            return Ok(json!({ "state": seen, "text": text }));
        }
        if let Some(thought) = frame["k"].as_str() {
            return Ok(json!({ "state": seen, "thinking": thought }));
        }
        if frame["fail"].is_string() {
            return Ok(json!({ "state": seen, "error": { "kind": "overloaded", "message": frame["fail"] } }));
        }
        if frame["kept"].as_bool() == Some(true) {
            return Ok(json!({ "state": seen, "done": true, "keep": { "thread": "t1" } }));
        }
        if frame["end"].as_bool() == Some(true) {
            let usage = json!({ "input": 4, "output": 2, "cache_read": seen });
            return Ok(json!({ "state": seen, "done": true, "usage": usage, "served": "model-x" }));
        }
        Ok(json!({ "state": seen }))
    }
}

static FRAMED: Framed = Framed;

fn request() -> TurnRequest {
    TurnRequest {
        model: "framed:fast".into(),
        system: "Be brief.".into(),
        messages: vec![
            Message::user("hi"),
            Message::assistant(vec![Block::Text("hello".into())]),
            Message::user("again"),
        ],
        tools: Vec::new(),
        effort: domain::Effort::Low,
        max_tokens: 100,
        intent: Default::default(),
    }
}

fn run(answers: Vec<(u16, &str)>) -> (Result<crate::model::Reply, ModelError>, Vec<Delta>, Arc<ScriptedHttp>) {
    let http = Arc::new(ScriptedHttp::new(answers.into_iter().map(|(status, text)| (status, text.to_owned()))));
    let api = Conversation::new(http.clone(), &FRAMED).without_backoff();
    let mut deltas = Vec::new();
    let reply = api.turn_blocking(&request(), &mut |delta| deltas.push(delta), &CancellationToken::new());
    (reply, deltas, http)
}

#[test]
fn the_body_carries_the_conversation_as_plain_turns() {
    let sent = body(&request());
    assert_eq!(sent["system"], "Be brief.");
    assert_eq!(sent["model"], "framed:fast");
    assert_eq!(
        sent["messages"],
        json!([{ "role": "user", "text": "hi" }, { "role": "assistant", "text": "hello" }, { "role": "user", "text": "again" }])
    );
}

#[test]
fn each_frame_goes_through_the_provider_until_it_says_done() {
    let stream = "{\"k\":\"hmm\"}\n{\"noise\":1}\n{\"t\":\"o\"}\n{\"t\":\"k\"}\n{\"end\":true}\n{\"t\":\"late\"}\n";
    let (reply, deltas, http) = run(vec![(200, stream)]);
    let Ok(reply) = reply else { panic!("{reply:?}") };
    assert_eq!(reply.message.text(), "ok");
    assert_eq!(reply.stop, StopReason::EndTurn);
    assert_eq!((reply.usage.input, reply.usage.output, reply.usage.cache_read), (4, 2, 5));
    assert_eq!(reply.served.as_deref(), Some("model-x"));
    assert_eq!(deltas, vec![Delta::Thinking("hmm".into()), Delta::Text("o".into()), Delta::Text("k".into())]);
    assert_eq!(http.sent().len(), 1);
}

#[test]
fn a_stream_that_ends_before_done_after_text_fails_without_a_retry() {
    let (reply, _, http) = run(vec![(200, "{\"t\":\"o\"}\n"); 4]);
    assert!(matches!(reply, Err(ModelError::Network(_))), "{reply:?}");
    assert_eq!(http.sent().len(), 1);
}

#[test]
fn an_error_the_provider_reads_in_a_frame_stops_the_turn() {
    let (reply, _, _) = run(vec![(200, "{\"fail\":\"busy\"}\n"); 4]);
    assert_eq!(reply.map(|reply| reply.stop), Err(ModelError::Overloaded("busy".into())));
}

#[test]
fn a_refused_socket_is_judged_by_its_status() {
    let (reply, _, http) = run(vec![(403, "expired")]);
    assert_eq!(reply.map(|reply| reply.stop), Err(ModelError::Unauthorized("expired".into())));
    assert_eq!(http.sent().len(), 1);
}

#[test]
fn what_the_provider_keeps_comes_back_on_the_next_turn_of_the_same_conversation() {
    let (first, _, http) = run(vec![(200, "{\"kept\":true}\n"), (200, "{\"end\":true}\n")]);
    assert!(first.is_ok(), "{first:?}");
    let api = Conversation::new(http.clone(), &FRAMED).without_backoff();
    let mut longer = request();
    longer.messages.push(Message::assistant(vec![Block::Text("fine".into())]));
    longer.messages.push(Message::user("bye"));
    let second = api.turn_blocking(&longer, &mut |_| {}, &CancellationToken::new());
    assert!(second.is_ok(), "{second:?}");
    let sent: Vec<Value> =
        http.sent().iter().map(|sent| serde_json::from_str(&sent.body).unwrap_or_default()).collect();
    assert_eq!(sent[0]["kept"], Value::Null);
    assert_eq!(sent[1]["kept"], json!({ "value": { "thread": "t1" }, "sent": 4 }));
    assert_eq!(sent[0]["conversation"], sent[1]["conversation"]);
}
