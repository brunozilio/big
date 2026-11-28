use serde_json::{Value, json};

use super::text;
use crate::redact::{Known, Redactor};
use crate::trace::runs::{Run, Span, SpanKind};

#[derive(Debug, Clone, Copy)]
pub struct Capture<'a> {
    pub redactor: &'a Redactor,
    pub limit: usize,
}

fn part(kind: &str, content: &str) -> Value {
    json!({"type": kind, "content": content})
}

impl Capture<'_> {
    fn cut(&self, text: String) -> String {
        match text.char_indices().nth(self.limit) {
            Some((at, _)) => format!("{} …", &text[..at]),
            None => text,
        }
    }

    fn clean(&self, said: &str) -> String {
        self.cut(self.redactor.redact(said, &Known::default()).text)
    }

    fn shortened(&self, value: Value) -> Value {
        match value {
            Value::String(said) => Value::String(self.cut(said)),
            Value::Array(all) => Value::Array(all.into_iter().map(|one| self.shortened(one)).collect()),
            Value::Object(all) => Value::Object(all.into_iter().map(|(key, one)| (key, self.shortened(one))).collect()),
            other => other,
        }
    }

    fn clean_value(&self, value: &Value) -> Value {
        self.shortened(self.redactor.redact_value(value, &Known::default()).0)
    }

    fn said(&self, key: &str, role: &str, parts: Vec<Value>) -> Option<Value> {
        (!parts.is_empty()).then(|| text(key, &json!([{"role": role, "parts": parts}]).to_string()))
    }

    fn texts(&self, pairs: &[(&str, &str)]) -> Vec<Value> {
        pairs
            .iter()
            .filter(|(_, said)| !said.trim().is_empty())
            .map(|(kind, said)| part(kind, &self.clean(said)))
            .collect()
    }

    fn sent(&self, run: &Run, span: &Span) -> Option<Value> {
        let request = span.request.as_ref()?;
        let messages: Vec<Value> =
            request.iter().filter_map(|at| run.messages.get(*at as usize)).map(|one| self.clean_value(one)).collect();
        (!messages.is_empty()).then(|| text("gen_ai.input.messages", &Value::Array(messages).to_string()))
    }

    pub fn attributes(&self, run: &Run, span: &Span) -> Vec<Value> {
        let mut attributes = Vec::new();
        match span.kind {
            SpanKind::Run | SpanKind::Agent => {
                attributes.extend(self.said("gen_ai.input.messages", "user", self.texts(&[("text", &span.input)])));
                attributes.extend(self.said(
                    "gen_ai.output.messages",
                    "assistant",
                    self.texts(&[("text", &span.output)]),
                ));
            }
            SpanKind::Llm => {
                attributes.extend(self.sent(run, span));
                let parts = self.texts(&[("reasoning", &span.thinking), ("text", &span.output)]);
                attributes.extend(self.said("gen_ai.output.messages", "assistant", parts));
            }
            SpanKind::Tool => {
                for (key, said) in
                    [("gen_ai.tool.call.arguments", &span.input), ("gen_ai.tool.call.result", &span.output)]
                {
                    if !said.trim().is_empty() {
                        attributes.push(text(key, &self.clean(said)));
                    }
                }
            }
            _ => {}
        }
        attributes
    }
}
