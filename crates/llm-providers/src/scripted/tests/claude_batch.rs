use std::sync::Arc;
use std::time::Duration;

use domain::ModelProvider;
use domain::provider::ProviderKey;
use serde_json::{Value, json};

use crate::batch::Windows;
use crate::model::client::ProviderClient;
use crate::model::http::ScriptedHttp;
use crate::model::{BlockingTurn, CancellationToken, Intent, Reply};
use crate::providers::testing::{credentials, request};
use crate::transcript::{Block, StopReason};

const CLAUDE: ModelProvider = ModelProvider::new("claude-subscription");
const MESSAGES: &str = include_str!("../../../tests/fixtures/messages_tool_use.sse");
const BATCHES: &str = "https://api.anthropic.com/v1/messages/batches";

fn quick(wait: u64) -> Windows {
    Windows { gather: Duration::from_millis(1), poll: Duration::from_millis(1), wait: Duration::from_millis(wait) }
}

fn client(http: &Arc<ScriptedHttp>, windows: Windows) -> ProviderClient {
    ProviderClient::new(CLAUDE, http.clone(), Arc::new(ProviderKey::new(credentials(CLAUDE)))).batching(windows)
}

fn ask(client: &ProviderClient, intent: Intent) -> Reply {
    let mut asked = request("claude-subscription:model-1");
    asked.intent = intent;
    client.turn_blocking(&asked, &mut |_| {}, &CancellationToken::new()).unwrap_or_else(|error| panic!("{error}"))
}

fn state(done: bool) -> String {
    let status = if done { "ended" } else { "in_progress" };
    json!({"id": "msgbatch_1", "processing_status": status, "results_url": format!("{BATCHES}/msgbatch_1/results")})
        .to_string()
}

fn succeeded(id: &str, text: &str) -> String {
    let message = json!({"id": "msg_1", "type": "message", "role": "assistant", "model": "model-1",
        "content": [
            {"type": "thinking", "thinking": "hm", "signature": "sig"},
            {"type": "text", "text": text},
            {"type": "tool_use", "id": "toolu_1", "name": "Read", "input": {"path": "a.rs"}}
        ],
        "stop_reason": "tool_use", "usage": {"input_tokens": 10, "output_tokens": 5}});
    json!({"custom_id": id, "result": {"type": "succeeded", "message": message}}).to_string()
}

fn body(http: &ScriptedHttp, at: usize) -> Value {
    serde_json::from_str(&http.sent()[at].body).unwrap_or_else(|error| panic!("{error}"))
}

#[test]
fn a_call_that_can_wait_goes_as_a_batch_and_folds_like_a_stream() {
    let results = format!("{}\n", succeeded("call-0", "the recap"));
    let http = Arc::new(ScriptedHttp::new([(200, state(false)), (200, state(true)), (200, results)]));
    let reply = ask(&client(&http, quick(5_000)), Intent::LATER);
    assert_eq!(reply.message.text(), "the recap");
    assert!(reply.batched, "the discount applies only to what a batch answered");
    assert_eq!(reply.stop, StopReason::ToolUse);
    assert_eq!((reply.usage.input, reply.usage.output), (10, 5));
    assert!(
        reply
            .message
            .blocks
            .iter()
            .any(|block| matches!(block, Block::ToolUse { input, .. } if input["path"] == "a.rs"))
    );
    let urls: Vec<String> = http.sent().iter().map(|sent| sent.url.clone()).collect();
    assert_eq!(urls, [BATCHES.to_owned(), format!("{BATCHES}/msgbatch_1"), format!("{BATCHES}/msgbatch_1/results")]);
    let submitted = body(&http, 0);
    assert_eq!(submitted["requests"][0]["custom_id"], "call-0");
    let params = &submitted["requests"][0]["params"];
    assert_eq!(params["model"], "model-1");
    assert!(params.get("stream").is_none() && params.get("intent").is_none());
}

#[test]
fn a_call_that_cannot_wait_never_batches() {
    let http = Arc::new(ScriptedHttp::new([(200, MESSAGES.to_owned())]));
    assert!(!ask(&client(&http, quick(5_000)), Intent::default()).batched);
    assert_eq!(http.sent().len(), 1);
    assert_eq!(http.sent()[0].url, "https://api.anthropic.com/v1/messages");
}

#[test]
fn a_refused_batch_falls_back_to_the_call_now() {
    let http = Arc::new(ScriptedHttp::new([(400, "{}".to_owned()), (200, MESSAGES.to_owned())]));
    let reply = ask(&client(&http, quick(5_000)), Intent::LATER);
    assert!(!reply.message.blocks.is_empty());
    assert!(!reply.batched, "a call that fell back is charged in full");
    assert_eq!(http.sent()[1].url, "https://api.anthropic.com/v1/messages");
}

#[test]
fn a_batch_that_does_not_end_in_time_is_cancelled_and_the_call_goes_now() {
    let http = Arc::new(ScriptedHttp::new([(200, state(false)), (200, "{}".to_owned()), (200, MESSAGES.to_owned())]));
    assert!(!ask(&client(&http, quick(0)), Intent::LATER).batched);
    let urls: Vec<String> = http.sent().iter().map(|sent| sent.url.clone()).collect();
    assert_eq!(urls[1], format!("{BATCHES}/msgbatch_1/cancel"));
    assert_eq!(urls[2], "https://api.anthropic.com/v1/messages");
}

#[test]
fn calls_that_wait_together_go_in_one_batch_and_each_gets_its_own_answer() {
    let results = format!("{}\n{}\n", succeeded("call-0", "first"), succeeded("call-1", "second"));
    let http = Arc::new(ScriptedHttp::new([(200, state(true)), (200, results)]));
    let windows = Windows { gather: Duration::from_millis(400), ..quick(5_000) };
    let answers: Vec<String> = std::thread::scope(|scope| {
        let asked: Vec<_> = (0..2)
            .map(|_| {
                let http = &http;
                scope.spawn(move || ask(&client(http, windows), Intent::LATER).message.text())
            })
            .collect();
        asked.into_iter().map(|each| each.join().unwrap_or_else(|_| panic!("asked"))).collect()
    });
    let mut sorted = answers.clone();
    sorted.sort();
    assert_eq!(sorted, ["first", "second"]);
    assert_eq!(http.sent().len(), 2);
    assert_eq!(body(&http, 0)["requests"].as_array().map(Vec::len), Some(2));
}

#[test]
fn claude_declares_the_discount_of_its_batches() {
    let claude = super::provider("claude-subscription");
    assert!(claude.batches());
    assert_eq!(claude.later_discount(), Some(0.5));
}
