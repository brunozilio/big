mod body;
mod stream;

use std::sync::Arc;
use std::time::Duration;

pub use body::{body, effort};
pub use stream::Folder;

use crate::model::http::Http;
use crate::model::retry::{Attempt, Failed, refusal, with_retries};
use crate::model::{BoxFuture, CancellationToken, Delta, ModelApi, ModelError, OnDelta, Reply, TurnRequest};

pub const NAME: &str = "messages";

pub const ORIGIN: crate::transcript::ReasoningOrigin = crate::transcript::ReasoningOrigin::of("Anthropic");

pub fn open(http: Arc<dyn Http>, _model: Option<&domain::ModelOption>) -> Arc<dyn ModelApi> {
    Arc::new(Messages::new(http))
}

pub struct Messages {
    http: Arc<dyn Http>,
    backoff: Duration,
}

impl std::fmt::Debug for Messages {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Messages").finish_non_exhaustive()
    }
}

impl Messages {
    pub fn new(http: Arc<dyn Http>) -> Self {
        Self { http, backoff: Duration::from_secs(2) }
    }

    pub fn without_backoff(mut self) -> Self {
        self.backoff = Duration::ZERO;
        self
    }
}

impl Attempt for Messages {
    fn once<'a>(
        &'a self,
        request: &'a TurnRequest,
        on: OnDelta<'a>,
        cancel: &'a CancellationToken,
    ) -> BoxFuture<'a, Result<Reply, Failed>> {
        Box::pin(async move {
            let response =
                self.http.post_streaming("", &[], &crate::model::intent::sent(request, body(request)), cancel).await?;
            if response.status != 200 {
                return Err(refusal(response, cancel, status_error).await);
            }
            Ok(crate::model::sse_read::read_sse(response, Folder::new(), on, cancel).await?)
        })
    }
}

impl crate::model::sse_read::SseFold for Folder {
    fn event(&mut self, data: &str, on: &mut dyn FnMut(Delta)) -> Result<(), ModelError> {
        Folder::event(self, data, on)
    }
    fn finish(self, _on: &mut dyn FnMut(Delta)) -> Result<Reply, ModelError> {
        Folder::finish(self)
    }
}

impl ModelApi for Messages {
    fn turn<'a>(
        &'a self,
        request: &'a TurnRequest,
        on: OnDelta<'a>,
        cancel: &'a CancellationToken,
    ) -> BoxFuture<'a, Result<Reply, ModelError>> {
        Box::pin(with_retries(NAME, self.backoff, self, request, on, cancel))
    }
}

fn status_error(status: u16, text: &str) -> ModelError {
    let parsed = serde_json::from_str::<serde_json::Value>(text).ok();
    match parsed.as_ref().map(|value| &value["error"]) {
        Some(error) if error.is_object() => stream::stream_error(error),
        _ => match status {
            401 | 403 => ModelError::Unauthorized(text.to_owned()),
            413 => ModelError::TooLong(text.to_owned()),
            429 | 500..=599 => ModelError::Overloaded(format!("HTTP {status}")),
            _ => ModelError::Provider(format!("HTTP {status}: {text}")),
        },
    }
}

#[cfg(test)]
mod wire_tests;
