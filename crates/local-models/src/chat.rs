use serde::Deserialize;
use serde_json::{Value, json};

use crate::LocalError;
use crate::engine::{self, Sampling, Shape};
use crate::output::{Format, Piece, Splitter};
use crate::store::resolve;
use crate::template::{Rendering, render};

#[derive(Debug, Clone, Default, Deserialize)]
struct Local {
    #[serde(default)]
    context: u32,
    #[serde(default)]
    gpu_layers: Option<u32>,
    #[serde(default)]
    formats: Vec<Format>,
    #[serde(default)]
    template_kwargs: Value,
    #[serde(default)]
    template: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct Ask {
    model: String,
    #[serde(default)]
    messages: Value,
    #[serde(default)]
    tools: Value,
    #[serde(default)]
    max_tokens: Option<usize>,
    #[serde(default)]
    temperature: Option<f32>,
    #[serde(default)]
    top_p: Option<f32>,
    #[serde(default)]
    top_k: Option<i32>,
    #[serde(default)]
    min_p: Option<f32>,
    #[serde(default)]
    repeat_penalty: Option<f32>,
    #[serde(default)]
    seed: Option<u32>,
    #[serde(default)]
    stop: Value,
    #[serde(default)]
    local: Local,
}

impl Ask {
    fn sampling(&self) -> Sampling {
        let base = Sampling::default();
        Sampling {
            temperature: self.temperature.unwrap_or(base.temperature),
            top_k: self.top_k.unwrap_or(base.top_k),
            top_p: self.top_p.unwrap_or(base.top_p),
            min_p: self.min_p.unwrap_or(base.min_p),
            repeat: self.repeat_penalty.unwrap_or(base.repeat),
            seed: self.seed.unwrap_or_else(rand_seed),
        }
    }

    fn stops(&self) -> Vec<String> {
        match &self.stop {
            Value::String(stop) => vec![stop.clone()],
            Value::Array(stops) => stops.iter().filter_map(Value::as_str).map(str::to_owned).collect(),
            _ => Vec::new(),
        }
    }

    fn shape(&self) -> Shape {
        Shape { context: self.local.context, gpu_layers: self.local.gpu_layers.unwrap_or(u32::MAX) }
    }
}

fn rand_seed() -> u32 {
    uuid::Uuid::new_v4().as_u128().to_le_bytes().iter().fold(0u32, |seed, byte| seed.rotate_left(5) ^ u32::from(*byte))
}

#[derive(Debug)]
pub struct Chunks {
    id: String,
    model: String,
    calls: usize,
}

impl Chunks {
    fn new(model: &str) -> Chunks {
        Chunks { id: format!("chatcmpl-{}", uuid::Uuid::new_v4().simple()), model: model.to_owned(), calls: 0 }
    }

    fn chunk(&self, delta: Value, finish: Option<&str>) -> Value {
        json!({
            "id": self.id, "object": "chat.completion.chunk", "model": self.model,
            "choices": [{ "index": 0, "delta": delta, "finish_reason": finish }],
        })
    }

    fn piece(&mut self, piece: Piece) -> Value {
        match piece {
            Piece::Reasoning(text) => self.chunk(json!({ "reasoning_content": text }), None),
            Piece::Content(text) => self.chunk(json!({ "content": text }), None),
            Piece::Call { name, arguments } => {
                let index = self.calls;
                self.calls += 1;
                let id = format!("call_{}", uuid::Uuid::new_v4().simple());
                let call = json!({ "index": index, "id": id, "type": "function", "function": { "name": name, "arguments": arguments } });
                self.chunk(json!({ "tool_calls": [call] }), None)
            }
        }
    }
}

pub fn complete(body: &Value, send: &mut dyn FnMut(Value) -> bool) -> Result<(), LocalError> {
    let ask: Ask = serde_json::from_value(body.clone()).map_err(|error| LocalError::Bad(error.to_string()))?;
    let path = resolve(&ask.model)?;
    engine::with_model(&path, ask.shape(), |model, own, session| {
        let template = ask.local.template.as_deref().filter(|template| !template.is_empty()).unwrap_or(own);
        if template.is_empty() {
            return Err(LocalError::Bad(format!("{} carries no chat template", ask.model)));
        }
        let format = Format::chosen(&ask.local.formats, template);
        let bos = engine::special(model, model.vocab().bos());
        let eos = engine::special(model, model.vocab().eos());
        let rendering = Rendering {
            template,
            messages: &ask.messages,
            tools: &ask.tools,
            extra: &ask.local.template_kwargs,
            bos: &bos,
            eos: &eos,
        };
        let prompt = render(&rendering).map_err(LocalError::Template)?;
        let thinking = format.think.as_ref().is_some_and(|think| prompt.trim_end().ends_with(think.open.trim_end()));
        let tokens = engine::tokens(model, &prompt);
        let reused = session.prefill(model, &tokens)?;
        let room = session.size().saturating_sub(tokens.len());
        let most = ask.max_tokens.map_or(room, |most| most.min(room));
        let mut splitter = Splitter::new(format, ask.tools.clone(), thinking, ask.stops());
        let mut chunks = Chunks::new(&ask.model);
        let mut open = true;
        let mut on = |text: &str| {
            let mut out = Vec::new();
            splitter.push(text, &mut out);
            for piece in out {
                open = open && send(chunks.piece(piece));
            }
            open && !splitter.stopped()
        };
        let (output, full) = session.generate(model, ask.sampling(), most, &mut on)?;
        let mut out = Vec::new();
        splitter.finish(&mut out);
        for piece in out {
            send(chunks.piece(piece));
        }
        let finish = if chunks.calls > 0 {
            "tool_calls"
        } else if full {
            "length"
        } else {
            "stop"
        };
        send(chunks.chunk(json!({}), Some(finish)));
        let usage = json!({
            "prompt_tokens": tokens.len(), "completion_tokens": output, "total_tokens": tokens.len() + output,
            "prompt_tokens_details": { "cached_tokens": reused },
        });
        send(
            json!({ "id": chunks.id, "object": "chat.completion.chunk", "model": ask.model, "choices": [], "usage": usage }),
        );
        Ok(())
    })
}

pub fn gathered(chunks: &[Value]) -> Value {
    let mut content = String::new();
    let mut reasoning = String::new();
    let mut calls = Vec::new();
    let mut finish = Value::Null;
    let mut usage = Value::Null;
    for chunk in chunks {
        let choice = &chunk["choices"][0];
        content.push_str(choice["delta"]["content"].as_str().unwrap_or_default());
        reasoning.push_str(choice["delta"]["reasoning_content"].as_str().unwrap_or_default());
        calls.extend(choice["delta"]["tool_calls"].as_array().into_iter().flatten().cloned());
        if !choice["finish_reason"].is_null() {
            finish = choice["finish_reason"].clone();
        }
        if chunk["usage"].is_object() {
            usage = chunk["usage"].clone();
        }
    }
    let mut message = json!({ "role": "assistant", "content": content });
    if !reasoning.is_empty() {
        message["reasoning_content"] = json!(reasoning);
    }
    if !calls.is_empty() {
        message["tool_calls"] = Value::Array(calls);
    }
    json!({ "object": "chat.completion", "choices": [{ "index": 0, "message": message, "finish_reason": finish }], "usage": usage })
}
