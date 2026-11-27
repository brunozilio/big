use serde_json::{Value, json};

use super::interception::{replace_text, rewritten_history, stop_label};
use crate::agent::Agent;
use crate::intercept::{Chain, Kind};
use crate::model::{ModelError, Reply, TurnRequest};
use crate::transcript::{Block, Message, Role, StopReason};

impl Agent<'_> {
    pub(super) fn answered(&self, request: &TurnRequest, shown: &mut String) -> Result<Reply, ModelError> {
        let interceptors = &self.workspace.interceptors;
        if !interceptors.listens_to(Kind::Model) {
            return self.streamed(request, shown);
        }
        let mut last: Option<Reply> = None;
        let mut failed: Option<ModelError> = None;
        let mut terminal = |asked: Value| {
            let changed = changed_request(request, &asked)?;
            match self.streamed(&changed, shown) {
                Ok(reply) => {
                    failed = None;
                    let answer = reply_value(&reply);
                    last = Some(reply);
                    Ok(answer)
                }
                Err(error) => {
                    let said = error.to_string();
                    failed = Some(error);
                    Err(said)
                }
            }
        };
        let chain = Chain {
            kind: Kind::Model,
            subject: &request.model,
            workspace: self.workspace,
            model: Some((self.api, &self.request)),
            terminal: &mut terminal,
        };
        match interceptors.run(request_value(request), chain) {
            Ok(answer) => reply_from(&answer, last),
            Err(error) => Err(failed.unwrap_or_else(|| refused(&error))),
        }
    }
}

fn refused(why: &str) -> ModelError {
    ModelError::Provider(format!("Interceptor failed: {why}"))
}

fn request_value(request: &TurnRequest) -> Value {
    let tools: Vec<&str> = request.tools.iter().map(|tool| tool.name.as_str()).collect();
    json!({
        "model": request.model,
        "system": request.system,
        "effort": request.effort.cli_value(),
        "maxTokens": request.max_tokens,
        "tools": tools,
        "history": request.messages,
    })
}

fn changed_request(original: &TurnRequest, asked: &Value) -> Result<TurnRequest, String> {
    let mut request = original.clone();
    if let Some(model) = asked["model"].as_str().filter(|model| !model.trim().is_empty()) {
        model.clone_into(&mut request.model);
    }
    if let Some(system) = asked["system"].as_str() {
        system.clone_into(&mut request.system);
    }
    if let Some(effort) = asked["effort"].as_str() {
        request.effort = domain::interception::effort(effort).ok_or_else(|| format!("unknown effort {effort:?}"))?;
    }
    if let Some(most) = asked["maxTokens"].as_u64() {
        request.max_tokens = u32::try_from(most).map_err(|_| format!("maxTokens {most} is too large"))?;
    }
    if let Some(names) = asked["tools"].as_array() {
        let names: Vec<&str> = names.iter().filter_map(Value::as_str).collect();
        request.tools.retain(|tool| names.contains(&tool.name.as_str()));
    }
    if let Some(history) = asked.get("history").filter(|history| !history.is_null()) {
        request.messages = rewritten_history(history.clone())?;
    }
    Ok(request)
}

fn reply_value(reply: &Reply) -> Value {
    let calls: Vec<Value> =
        reply.message.tool_uses().map(|(id, name, input)| json!({ "id": id, "name": name, "input": input })).collect();
    json!({
        "text": reply.message.text(),
        "toolCalls": calls,
        "stopReason": stop_label(reply.stop),
        "usage": { "input": reply.usage.input, "output": reply.usage.output },
        "message": reply.message,
    })
}

fn reply_from(answer: &Value, last: Option<Reply>) -> Result<Reply, ModelError> {
    let Some(mut reply) = last.or_else(|| built(answer)) else {
        return Err(refused("it returned no reply and never called next"));
    };
    let before = reply.message.text();
    if let Some(message) = answer.get("message").filter(|message| !message.is_null()) {
        match serde_json::from_value::<Message>(message.clone()) {
            Ok(message) => reply.message = message,
            Err(error) => tracing::warn!(%error, "an interceptor returned a message that was left out"),
        }
    }
    if let Some(text) = answer["text"].as_str().filter(|text| *text != before) {
        replace_text(&mut reply.message.blocks, text.to_owned());
    }
    Ok(reply)
}

fn built(answer: &Value) -> Option<Reply> {
    let mut blocks = Vec::new();
    if let Some(text) = answer["text"].as_str().filter(|text| !text.is_empty()) {
        blocks.push(Block::Text(text.to_owned()));
    }
    for (at, call) in answer["toolCalls"].as_array().into_iter().flatten().enumerate() {
        let name = call["name"].as_str()?.to_owned();
        let id = call["id"].as_str().map_or_else(|| format!("intercepted-{at}"), str::to_owned);
        blocks.push(Block::ToolUse { id, name, input: call["input"].clone() });
    }
    if blocks.is_empty() && answer.get("message").is_none_or(Value::is_null) {
        return None;
    }
    let calls = blocks.iter().any(|block| matches!(block, Block::ToolUse { .. }));
    Some(Reply {
        message: Message { role: Role::Assistant, blocks },
        stop: if calls { StopReason::ToolUse } else { StopReason::EndTurn },
        usage: Default::default(),
        served: None,
        account: None,
        batched: false,
    })
}
