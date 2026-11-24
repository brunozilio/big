use serde_json::{Map, Value, json};

use super::http::Body;
use super::sse::SseDecoder;
use super::{CancellationToken, ModelError, event_stream};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Framing {
    Sse,
    Ndjson,
    EventStream,
    Raw,
}

impl Framing {
    pub fn named(name: &str) -> Result<Self, String> {
        match name {
            "sse" => Ok(Self::Sse),
            "ndjson" => Ok(Self::Ndjson),
            "aws-eventstream" => Ok(Self::EventStream),
            "raw" => Ok(Self::Raw),
            other => Err(format!("unknown framing {other}; use sse, ndjson, aws-eventstream or raw")),
        }
    }
}

#[derive(Debug)]
pub struct Framed {
    body: Body,
    framing: Framing,
    sse: SseDecoder,
    held: Vec<u8>,
    ended: bool,
}

impl Framed {
    pub fn new(body: Body, framing: Framing) -> Self {
        Self { body, framing, sse: SseDecoder::new(), held: Vec::new(), ended: false }
    }

    pub async fn next(&mut self, cancel: &CancellationToken) -> Result<Option<Value>, ModelError> {
        if self.ended {
            return Ok(None);
        }
        if cancel.is_cancelled() {
            return Err(ModelError::Cancelled);
        }
        let next = match self.framing {
            Framing::Sse => self.sse(cancel).await,
            Framing::Ndjson => self.ndjson(cancel).await,
            Framing::EventStream => self.frame(cancel).await,
            Framing::Raw => self.raw(cancel).await,
        };
        if !matches!(next, Ok(Some(_))) {
            self.ended = true;
        }
        next
    }

    async fn sse(&mut self, cancel: &CancellationToken) -> Result<Option<Value>, ModelError> {
        let mut line = String::new();
        loop {
            line.clear();
            let read = self.body.read_line(&mut line, cancel).await?;
            let event = if read == 0 { self.sse.finish() } else { self.sse.line(line.trim_end_matches('\n'))? };
            if let Some(event) = event {
                return Ok(Some(json!({ "event": event.event, "data": event.data })));
            }
            if read == 0 {
                return Ok(None);
            }
        }
    }

    async fn ndjson(&mut self, cancel: &CancellationToken) -> Result<Option<Value>, ModelError> {
        let mut line = String::new();
        loop {
            line.clear();
            if self.body.read_line(&mut line, cancel).await? == 0 {
                return Ok(None);
            }
            let text = line.trim();
            if text.is_empty() {
                continue;
            }
            return serde_json::from_str(text)
                .map(Some)
                .map_err(|error| ModelError::Network(format!("a line of the stream is not JSON: {error}")));
        }
    }

    async fn frame(&mut self, cancel: &CancellationToken) -> Result<Option<Value>, ModelError> {
        let Some(frame) = event_stream::next(&mut self.body, cancel).await? else {
            return Ok(None);
        };
        let headers: Map<String, Value> =
            frame.headers.into_iter().map(|(name, value)| (name, Value::String(value))).collect();
        Ok(Some(json!({ "headers": headers, "payload": String::from_utf8_lossy(&frame.payload) })))
    }

    async fn raw(&mut self, cancel: &CancellationToken) -> Result<Option<Value>, ModelError> {
        loop {
            let data = self.body.fill(cancel).await?;
            if data.is_empty() {
                if self.held.is_empty() {
                    return Ok(None);
                }
                let rest = String::from_utf8_lossy(&std::mem::take(&mut self.held)).into_owned();
                return Ok(Some(Value::String(rest)));
            }
            let count = data.len();
            self.held.extend_from_slice(data);
            self.body.consume_some(count);
            let whole = match std::str::from_utf8(&self.held) {
                Ok(_) => self.held.len(),
                Err(error) if error.error_len().is_some() => self.held.len(),
                Err(error) => error.valid_up_to(),
            };
            if whole > 0 {
                let text = String::from_utf8_lossy(&self.held[..whole]).into_owned();
                self.held.drain(..whole);
                return Ok(Some(Value::String(text)));
            }
        }
    }
}

#[cfg(test)]
#[path = "framing_tests.rs"]
mod tests;
