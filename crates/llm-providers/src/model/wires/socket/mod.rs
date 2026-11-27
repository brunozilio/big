use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use serde_json::{Value, json};

use crate::Provider;
use crate::model::conversation_key::conversation_key;
use crate::model::framing::{Framed, Framing};
use crate::model::http::Http;
use crate::model::retry::{Attempt, Failed, refusal, with_retries};
use crate::model::{BoxFuture, CancellationToken, Delta, ModelApi, ModelError, OnDelta, Reply, TurnRequest};
use crate::transcript::{Block, Message, Role, StopReason, Usage};

pub const NAME: &str = "socket";
const KEPT_AT_MOST: usize = 256;

#[derive(Debug, Clone)]
struct Kept {
    value: Value,
    sent: usize,
}

type Remembered = HashMap<(&'static str, uuid::Uuid), Kept>;

static KEPT: Mutex<Option<Remembered>> = Mutex::new(None);

fn recalled(provider: &'static str, conversation: uuid::Uuid) -> Option<Kept> {
    let kept = KEPT.lock().unwrap_or_else(PoisonError::into_inner);
    kept.as_ref().and_then(|kept| kept.get(&(provider, conversation)).cloned())
}

fn remember(provider: &'static str, conversation: uuid::Uuid, value: Value, sent: usize) {
    let mut kept = KEPT.lock().unwrap_or_else(PoisonError::into_inner);
    let kept = kept.get_or_insert_with(HashMap::new);
    if kept.len() >= KEPT_AT_MOST && !kept.contains_key(&(provider, conversation)) {
        kept.clear();
    }
    kept.insert((provider, conversation), Kept { value, sent });
}

pub fn open(http: Arc<dyn Http>, provider: &'static dyn Provider) -> Arc<dyn ModelApi> {
    Arc::new(Conversation::new(http, provider))
}

pub struct Conversation {
    http: Arc<dyn Http>,
    provider: &'static dyn Provider,
    backoff: Duration,
}

impl std::fmt::Debug for Conversation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Conversation").field("provider", &self.provider.id()).finish_non_exhaustive()
    }
}

impl Conversation {
    pub fn new(http: Arc<dyn Http>, provider: &'static dyn Provider) -> Self {
        Self { http, provider, backoff: Duration::from_secs(2) }
    }

    #[must_use]
    pub fn without_backoff(mut self) -> Self {
        self.backoff = Duration::ZERO;
        self
    }
}

fn said(message: &Message) -> String {
    let parts: Vec<&str> = message
        .blocks
        .iter()
        .filter_map(|block| match block {
            Block::Text(text) => Some(text.as_str()),
            Block::ToolResultRich { content, .. } => Some(content.as_str()),
            _ => None,
        })
        .filter(|text| !text.is_empty())
        .collect();
    parts.join("\n\n")
}

fn body_kept(request: &TurnRequest, conversation: uuid::Uuid, kept: Option<&Kept>) -> Value {
    let mut sent = body(request);
    sent["conversation"] = json!(conversation.to_string());
    sent["kept"] = kept.map_or(Value::Null, |kept| json!({ "value": kept.value, "sent": kept.sent }));
    sent
}

pub fn body(request: &TurnRequest) -> Value {
    let messages: Vec<Value> = request
        .messages
        .iter()
        .map(|message| {
            let role = match message.role {
                Role::User => "user",
                Role::Assistant => "assistant",
            };
            json!({ "role": role, "text": said(message) })
        })
        .collect();
    let tools: Vec<Value> = request
        .tools
        .iter()
        .map(|tool| json!({ "name": tool.name, "description": tool.description, "schema": tool.schema }))
        .collect();
    json!({
        "model": request.model,
        "system": request.system,
        "messages": messages,
        "tools": tools,
        "effort": request.effort,
        "max_tokens": request.max_tokens,
    })
}

fn count(value: &Value, name: &str) -> u64 {
    value[name].as_u64().unwrap_or_default()
}

fn error_of(found: &Value) -> ModelError {
    let message = found["message"].as_str().unwrap_or("the provider failed the turn").to_owned();
    match found["kind"].as_str().unwrap_or_default() {
        "unauthorized" => ModelError::Unauthorized(message),
        "overloaded" => ModelError::Overloaded(message),
        "too_long" => ModelError::TooLong(message),
        _ => ModelError::Provider(message),
    }
}

#[derive(Debug, Default)]
struct Folded {
    text: String,
    thinking: String,
    usage: Usage,
    served: Option<String>,
    stop: Option<StopReason>,
    keep: Option<Value>,
}

impl Folded {
    fn take(&mut self, out: &Value, on: OnDelta<'_>) -> Result<bool, ModelError> {
        if out["error"].is_object() {
            return Err(error_of(&out["error"]));
        }
        if let Some(thinking) = out["thinking"].as_str().filter(|text| !text.is_empty()) {
            self.thinking.push_str(thinking);
            on(Delta::Thinking(thinking.to_owned()));
        }
        if let Some(text) = out["text"].as_str().filter(|text| !text.is_empty()) {
            self.text.push_str(text);
            on(Delta::Text(text.to_owned()));
        }
        if out["usage"].is_object() {
            let usage = &out["usage"];
            self.usage = Usage {
                input: count(usage, "input"),
                output: count(usage, "output"),
                cache_read: count(usage, "cache_read"),
                cache_write: count(usage, "cache_write"),
                searches: count(usage, "searches"),
            };
        }
        if let Some(served) = out["served"].as_str().filter(|served| !served.is_empty()) {
            self.served = Some(served.to_owned());
        }
        if !out["keep"].is_null() {
            self.keep = Some(out["keep"].clone());
        }
        if out["stop"].as_str() == Some("max_tokens") {
            self.stop = Some(StopReason::MaxTokens);
        }
        Ok(out["done"].as_bool().unwrap_or(false))
    }

    fn reply(self) -> Reply {
        let mut blocks = Vec::new();
        if !self.thinking.is_empty() {
            blocks.push(Block::Thinking { text: self.thinking, signature: None, origin: None });
        }
        blocks.push(Block::Text(self.text));
        Reply {
            message: Message::assistant(blocks),
            stop: self.stop.unwrap_or(StopReason::EndTurn),
            usage: self.usage,
            served: self.served,
            account: None,
            batched: false,
        }
    }
}

fn status_error(status: u16, text: &str) -> ModelError {
    match status {
        401 | 403 => ModelError::Unauthorized(text.to_owned()),
        429 | 500..=599 => ModelError::Overloaded(format!("HTTP {status}: {text}")),
        _ => ModelError::Provider(format!("HTTP {status}: {text}")),
    }
}

impl Attempt for Conversation {
    fn once<'a>(
        &'a self,
        request: &'a TurnRequest,
        on: OnDelta<'a>,
        cancel: &'a CancellationToken,
    ) -> BoxFuture<'a, Result<Reply, Failed>> {
        Box::pin(async move {
            let framing = Framing::named(self.provider.framing()).map_err(ModelError::Provider)?;
            let conversation = conversation_key(request);
            let kept = recalled(self.provider.id(), conversation);
            let sent = body_kept(request, conversation, kept.as_ref()).to_string();
            let response = self.http.post_streaming("", &[], &sent, cancel).await?;
            if response.status != 200 {
                return Err(refusal(response, cancel, status_error).await);
            }
            let mut framed = Framed::new(response.body, framing);
            let mut state = Value::Null;
            let mut folded = Folded::default();
            while let Some(frame) = framed.next(cancel).await? {
                let out = self.provider.frame(&state, &frame).map_err(ModelError::from)?;
                state = out["state"].clone();
                if folded.take(&out, on)? {
                    if let Some(keep) = folded.keep.take() {
                        remember(self.provider.id(), conversation, keep, request.messages.len() + 1);
                    }
                    return Ok(folded.reply());
                }
            }
            Err(ModelError::Network("the stream ended before the turn finished".into()).into())
        })
    }
}

impl ModelApi for Conversation {
    fn turn<'a>(
        &'a self,
        request: &'a TurnRequest,
        on: OnDelta<'a>,
        cancel: &'a CancellationToken,
    ) -> BoxFuture<'a, Result<Reply, ModelError>> {
        Box::pin(with_retries(self.provider.name(), self.backoff, self, request, on, cancel))
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
