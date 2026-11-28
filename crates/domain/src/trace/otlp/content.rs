use serde_json::{Value, json};

use super::text;
use crate::redact::{Known, Redactor};
use crate::trace::runs::{Run, Span, SpanKind};

#[derive(Debug, Clone, Copy)]
pub struct Capture<'a> {
    pub redactor: &'a Redactor,
    pub known: &'a Known,
    pub limit: usize,
}

fn part(kind: &str, content: &str) -> Value {
    json!({"type": kind, "content": content})
}

fn budgeted(value: Value, left: &mut usize) -> Value {
    match value {
        Value::String(said) => {
            let kept = match said.char_indices().nth(*left) {
                Some((at, _)) => format!("{} …", &said[..at]),
                None => said.clone(),
            };
            *left = left.saturating_sub(said.chars().count());
            Value::String(kept)
        }
        Value::Array(all) => Value::Array(all.into_iter().map(|one| budgeted(one, left)).collect()),
        Value::Object(all) => Value::Object(all.into_iter().map(|(key, one)| (key, budgeted(one, left))).collect()),
        other => other,
    }
}

impl Capture<'_> {
    fn fitted(&self, items: Vec<Value>, reserved: usize) -> Vec<Value> {
        let mut left = self.limit.saturating_sub(reserved);
        let mut kept: Vec<Value> = Vec::new();
        for item in items.into_iter().rev() {
            if left == 0 {
                break;
            }
            kept.push(budgeted(item, &mut left));
        }
        kept.reverse();
        kept
    }

    fn clean(&self, said: &str) -> Value {
        Value::String(self.redactor.redact(said, self.known).text)
    }

    fn said(&self, key: &str, role: &str, parts: Vec<Value>) -> Option<Value> {
        let parts = self.fitted(parts, role.chars().count());
        (!parts.is_empty()).then(|| text(key, &json!([{"role": role, "parts": parts}]).to_string()))
    }

    fn texts(&self, pairs: &[(&str, &str)]) -> Vec<Value> {
        let said = pairs.iter().filter(|(_, said)| !said.trim().is_empty());
        said.map(|(kind, said)| part(kind, self.clean(said).as_str().unwrap_or_default())).collect()
    }

    fn sent(&self, run: &Run, span: &Span) -> Option<Value> {
        let request = span.request.as_ref()?;
        let messages = request.iter().filter_map(|at| run.messages.get(*at as usize));
        let messages = self.fitted(messages.map(|one| self.redactor.redact_value(one, self.known).0).collect(), 0);
        (!messages.is_empty()).then(|| text("gen_ai.input.messages", &Value::Array(messages).to_string()))
    }

    pub fn attributes(&self, run: &Run, span: &Span) -> Vec<Value> {
        let mut attributes = Vec::new();
        match span.kind {
            SpanKind::Run | SpanKind::Agent => {
                attributes.extend(self.said("gen_ai.input.messages", "user", self.texts(&[("text", &span.input)])));
                let answer = self.texts(&[("text", &span.output)]);
                attributes.extend(self.said("gen_ai.output.messages", "assistant", answer));
            }
            SpanKind::Llm => {
                attributes.extend(self.sent(run, span));
                let parts = self.texts(&[("reasoning", &span.thinking), ("text", &span.output)]);
                attributes.extend(self.said("gen_ai.output.messages", "assistant", parts));
            }
            SpanKind::Tool => {
                let pairs = [("gen_ai.tool.call.arguments", &span.input), ("gen_ai.tool.call.result", &span.output)];
                for (key, said) in pairs.into_iter().filter(|(_, said)| !said.trim().is_empty()) {
                    let kept = self.fitted(vec![self.clean(said)], 0);
                    attributes.extend(kept.first().and_then(Value::as_str).map(|kept| text(key, kept)));
                }
            }
            _ => {}
        }
        attributes
    }
}
