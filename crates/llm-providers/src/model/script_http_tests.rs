use std::sync::Mutex;

use serde_json::{Value, json};

use super::*;
use crate::model::BoxFuture;
use crate::model::http::{Body, ScriptedHttp};

fn run<T: Send>(work: impl std::future::Future<Output = T> + Send) -> T {
    match platform::runtime::block_on(work) {
        Ok(done) => done,
        Err(error) => panic!("runtime: {error}"),
    }
}

type Seen = (String, String, Option<String>, bool);

#[derive(Default)]
struct Recorder {
    seen: Mutex<Vec<Seen>>,
    answer: Mutex<Option<(u16, Vec<u8>)>>,
}

impl Recorder {
    fn answering(status: u16, body: &[u8]) -> Self {
        Self { answer: Mutex::new(Some((status, body.to_vec()))), ..Self::default() }
    }
}

impl Http for Recorder {
    fn send<'a>(
        &'a self,
        out: Outgoing<'a>,
        _cancel: &'a CancellationToken,
    ) -> BoxFuture<'a, Result<Response, ModelError>> {
        if let Ok(mut seen) = self.seen.lock() {
            seen.push((out.method.to_owned(), out.url.to_owned(), out.body.map(str::to_owned), out.streaming));
        }
        let answer = self.answer.lock().ok().and_then(|mut answer| answer.take());
        Box::pin(async move {
            let (status, body) = answer.ok_or_else(|| ModelError::Network("no answer".into()))?;
            let headers = vec![("X-Id".to_owned(), "1".to_owned()), ("x-id".to_owned(), "2".to_owned())];
            Ok(Response { status, headers, body: Body::from(body) })
        })
    }

    fn post(&self, _url: &str, _headers: &[(String, String)], _body: &str) -> Result<Response, String> {
        Err("not used".into())
    }
}

#[test]
fn a_request_keeps_its_method_body_and_answers_with_headers() {
    let http = Recorder::answering(201, b"done");
    let arguments = json!({ "method": "put", "url": "https://api.test/x", "body": { "a": 1 } });
    let answer = run(request(&http, &arguments, &CancellationToken::new()));
    assert_eq!(answer, Ok(json!({ "status": 201, "headers": { "x-id": "1, 2" }, "body": "done" })));
    let seen = http.seen.lock().map(|seen| seen.clone()).unwrap_or_default();
    assert_eq!(seen, [("PUT".to_owned(), "https://api.test/x".to_owned(), Some("{\"a\":1}".to_owned()), false)]);
}

#[test]
fn a_request_reads_binary_as_base64_and_stops_at_the_cap() {
    let http = Recorder::answering(200, &[0, 255]);
    let arguments = json!({ "url": "https://api.test/b", "encoding": "base64" });
    let answer = run(request(&http, &arguments, &CancellationToken::new())).unwrap_or_default();
    assert_eq!(answer["body"], "AP8=");
    let http = Recorder::answering(200, b"0123456789");
    let arguments = json!({ "url": "https://api.test/b", "most": 4 });
    let answer = run(request(&http, &arguments, &CancellationToken::new())).unwrap_or_default();
    assert_eq!(answer["body"], "0123");
}

#[test]
fn a_stream_is_read_event_by_event_and_closes_at_its_end() {
    let http = ScriptedHttp::new([(200, "data: one\n\ndata: two\n\n".to_owned())]);
    let streams = Streams::default();
    let cancel = CancellationToken::new();
    let opened = json!({ "url": "https://api.test/s", "body": "{}", "framing": "sse" });
    let open = run(streams.call(&http, "stream.open", &opened, &cancel)).unwrap_or_default();
    assert_eq!(open["status"], 200);
    let id = open["id"].clone();
    let next = || run(streams.call(&http, "stream.next", &id, &cancel));
    assert_eq!(next(), Ok(json!({ "event": "", "data": "one" })));
    assert_eq!(next(), Ok(json!({ "event": "", "data": "two" })));
    assert_eq!(next(), Ok(Value::Null));
    assert!(next().is_err());
    assert_eq!(http.sent()[0].body, "{}");
}

#[test]
fn a_failed_stream_answers_its_status_and_body_without_opening() {
    let http = ScriptedHttp::new([(429, "slow down".to_owned())]);
    let streams = Streams::default();
    let opened = json!({ "url": "https://api.test/s", "body": "{}" });
    let open = run(streams.open(&http, &opened, &CancellationToken::new())).unwrap_or_default();
    assert_eq!(open["id"], Value::Null);
    assert_eq!(open["status"], 429);
    assert_eq!(open["body"], "slow down");
}

#[test]
fn streams_close_and_are_limited() {
    let answers = (0..=OPEN_AT_MOST).map(|_| (200, "data: x\n\n".to_owned()));
    let http = ScriptedHttp::new(answers);
    let streams = Streams::default();
    let cancel = CancellationToken::new();
    let opened = json!({ "url": "https://api.test/s", "body": "{}", "framing": "sse" });
    let ids: Vec<Value> = (0..OPEN_AT_MOST)
        .map(|_| run(streams.open(&http, &opened, &cancel)).unwrap_or_default()["id"].clone())
        .collect();
    assert!(run(streams.open(&http, &opened, &cancel)).is_err());
    assert_eq!(streams.close(&ids[0]), Ok(Value::Bool(true)));
    assert_eq!(streams.close(&ids[0]), Ok(Value::Bool(false)));
    assert!(run(streams.open(&http, &opened, &cancel)).is_ok());
    assert!(run(streams.call(&http, "stream.open", &json!({ "url": "u", "framing": "xml" }), &cancel)).is_err());
    assert!(run(streams.call(&http, "nothing", &json!({}), &cancel)).is_err());
}

#[test]
fn a_cancelled_read_reports_cancellation() {
    let http = ScriptedHttp::new([(200, "data: x\n\n".to_owned())]);
    let streams = Streams::default();
    let cancel = CancellationToken::new();
    let opened = json!({ "url": "https://api.test/s", "body": "{}" });
    let id = run(streams.open(&http, &opened, &cancel)).unwrap_or_default()["id"].clone();
    cancel.cancel();
    assert_eq!(run(streams.next(&id, &cancel)), Err(ModelError::Cancelled.to_string()));
}
