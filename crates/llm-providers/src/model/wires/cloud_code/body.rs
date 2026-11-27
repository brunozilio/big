use std::collections::HashMap;

use domain::Effort;
use serde_json::{Map, Value, json};

use crate::model::{ToolSpec, TurnRequest};
use crate::transcript::{Block, Message, Role};

pub const SKIP_SIGNATURE: &str = "skip_thought_signature_validator";

pub fn body(request: &TurnRequest) -> Value {
    let key = crate::model::conversation_key::conversation_key(request);
    let mut inner = json!({
        "contents": contents(&request.messages),
        "generationConfig": {
            "maxOutputTokens": request.max_tokens,
            "thinkingConfig": {"includeThoughts": true, "thinkingLevel": level(request.effort)},
        },
        "sessionId": key.to_string(),
    });
    if !request.system.trim().is_empty() {
        inner["systemInstruction"] = json!({"role": "user", "parts": [{"text": request.system}]});
    }
    if !request.tools.is_empty() {
        inner["tools"] = json!([{"functionDeclarations": request.tools.iter().map(declaration).collect::<Vec<_>>()}]);
    }
    json!({
        "model": request.model,
        "project": "",
        "requestId": uuid::Uuid::new_v4().to_string(),
        "request": inner,
    })
}

pub const fn level(effort: Effort) -> &'static str {
    match effort {
        Effort::Low => "low",
        Effort::Medium => "medium",
        Effort::High | Effort::XHigh | Effort::Max => "high",
    }
}

fn declaration(tool: &ToolSpec) -> Value {
    let mut schema = clean(&tool.schema);
    if !schema.is_object() {
        schema = json!({});
    }
    schema["type"] = json!("object");
    if schema["properties"].as_object().is_none_or(Map::is_empty) {
        schema["properties"] =
            json!({"_placeholder": {"type": "boolean", "description": "Placeholder. Always pass true."}});
    }
    json!({"name": tool.name, "description": tool.description, "parametersJsonSchema": schema})
}

fn clean(schema: &Value) -> Value {
    match schema {
        Value::Object(fields) => Value::Object(
            fields
                .iter()
                .filter(|(key, _)| !key.starts_with('$'))
                .map(|(key, value)| (key.clone(), clean(value)))
                .collect(),
        ),
        Value::Array(items) => Value::Array(items.iter().map(clean).collect()),
        other => other.clone(),
    }
}

fn contents(messages: &[Message]) -> Vec<Value> {
    let names: HashMap<&str, &str> =
        messages.iter().flat_map(Message::tool_uses).map(|(id, name, _)| (id, name)).collect();
    let mut contents: Vec<Value> = messages
        .iter()
        .filter_map(|message| {
            let parts = match message.role {
                Role::User => user(message, &names),
                Role::Assistant => model(message),
            };
            let role = if message.role == Role::User { "user" } else { "model" };
            (!parts.is_empty()).then(|| json!({"role": role, "parts": parts}))
        })
        .collect();
    if contents.last().is_none_or(|last| last["role"] != "user") {
        contents.push(json!({"role": "user", "parts": [{"text": "Continue."}]}));
    }
    contents
}

fn user(message: &Message, names: &HashMap<&str, &str>) -> Vec<Value> {
    let mut parts = Vec::new();
    for block in &message.blocks {
        match block {
            Block::Text(_) | Block::Reminder(_) => {
                parts.extend(block.sent_text().filter(|text| !text.is_empty()).map(|text| json!({"text": text})));
            }
            Block::Image(image) => parts.push(inline(image)),
            Block::ToolResult { id, content, is_error } => parts.push(answer(id, content, *is_error, names)),
            Block::ToolResultRich { id, content, images, is_error } => {
                parts.push(answer(id, content, *is_error, names));
                parts.extend(images.iter().map(inline));
            }
            Block::Thinking { .. } | Block::ToolUse { .. } | Block::Native { .. } => {}
        }
    }
    parts
}

fn model(message: &Message) -> Vec<Value> {
    let mut parts = Vec::new();
    let mut pending: Option<String> = None;
    let mut called = false;
    for block in &message.blocks {
        match block {
            Block::Thinking { text, signature, origin } if *origin == Some(super::ORIGIN) => {
                if text.is_empty() {
                    pending.clone_from(signature);
                } else {
                    let signature = signature.as_deref().unwrap_or(SKIP_SIGNATURE);
                    parts.push(json!({"thought": true, "text": text, "thoughtSignature": signature}));
                }
            }
            Block::Text(text) if !text.is_empty() => {
                let mut part = json!({"text": text});
                if let Some(signature) = pending.take() {
                    part["thoughtSignature"] = json!(signature);
                }
                parts.push(part);
            }
            Block::ToolUse { id, name, input } => {
                let mut part = json!({"functionCall": {"id": id, "name": name, "args": input}});
                match pending.take() {
                    Some(signature) => part["thoughtSignature"] = json!(signature),
                    None if !called => part["thoughtSignature"] = json!(SKIP_SIGNATURE),
                    None => {}
                }
                called = true;
                parts.push(part);
            }
            _ => {}
        }
    }
    parts
}

fn answer(id: &str, content: &str, is_error: bool, names: &HashMap<&str, &str>) -> Value {
    let name = names.get(id).copied().unwrap_or("unknown");
    let response = if is_error { json!({"error": content}) } else { json!({"output": content}) };
    json!({"functionResponse": {"id": id, "name": name, "response": response}})
}

fn inline(image: &domain::Image) -> Value {
    json!({"inlineData": {"mimeType": image.media_type, "data": image.data}})
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(model: &str, messages: Vec<Message>) -> TurnRequest {
        TurnRequest {
            model: format!("antigravity-subscription:{model}"),
            system: "Be terse.".into(),
            messages,
            tools: vec![ToolSpec {
                name: "Read".into(),
                description: "Reads".into(),
                schema: json!({"$schema": "x", "type": "object", "properties": {"file_path": {"type": "string"}}}),
            }],
            effort: Effort::Medium,
            max_tokens: 8192,
            intent: Default::default(),
        }
    }

    fn signed(text: &str, signature: Option<&str>) -> Block {
        Block::Thinking {
            text: text.into(),
            signature: signature.map(Into::into),
            origin: Some(crate::model::wires::cloud_code::ORIGIN),
        }
    }

    #[test]
    fn a_first_turn_carries_the_system_prompt_the_tools_and_the_envelope() {
        let body = body(&request("gemini-3.8-flash-high", vec![Message::user("What is in a.txt?")]));
        assert_eq!(body["model"], "antigravity-subscription:gemini-3.8-flash-high");
        assert_eq!(body["project"], "");
        assert!(body["requestId"].as_str().is_some_and(|id| uuid::Uuid::parse_str(id).is_ok()));
        assert!(body.get("userAgent").is_none() && body.get("requestType").is_none(), "the provider names its client");
        let inner = &body["request"];
        assert_eq!(inner["systemInstruction"], json!({"role": "user", "parts": [{"text": "Be terse."}]}));
        assert_eq!(inner["contents"], json!([{"role": "user", "parts": [{"text": "What is in a.txt?"}]}]));
        let declared = &inner["tools"][0]["functionDeclarations"][0];
        assert_eq!(declared["name"], "Read");
        assert_eq!(declared["parametersJsonSchema"]["properties"]["file_path"]["type"], "string");
        assert!(declared["parametersJsonSchema"].get("$schema").is_none());
        assert!(inner.get("toolConfig").is_none(), "the provider decides which models are held to declared calls");
        assert_eq!(
            inner["generationConfig"],
            json!({"maxOutputTokens": 8192, "thinkingConfig": {"includeThoughts": true, "thinkingLevel": "medium"}})
        );
    }

    #[test]
    fn calls_and_results_are_paired_by_id_and_name() {
        let body = body(&request(
            "gemini-3.8-flash-high",
            vec![
                Message::user("Read a"),
                Message::assistant(vec![
                    signed("plan", None),
                    Block::Text("Reading.".into()),
                    Block::ToolUse { id: "t1".into(), name: "Read".into(), input: json!({"file_path": "a"}) },
                    Block::ToolUse { id: "t2".into(), name: "Read".into(), input: json!({"file_path": "b"}) },
                ]),
                Message {
                    role: Role::User,
                    blocks: vec![
                        Block::ToolResult { id: "t1".into(), content: "pineapple".into(), is_error: false },
                        Block::ToolResult { id: "t2".into(), content: "missing".into(), is_error: true },
                    ],
                },
            ],
        ));
        let contents = &body["request"]["contents"];
        assert_eq!(
            contents[1],
            json!({"role": "model", "parts": [
                {"thought": true, "text": "plan", "thoughtSignature": SKIP_SIGNATURE},
                {"text": "Reading."},
                {"functionCall": {"id": "t1", "name": "Read", "args": {"file_path": "a"}}, "thoughtSignature": SKIP_SIGNATURE},
                {"functionCall": {"id": "t2", "name": "Read", "args": {"file_path": "b"}}},
            ]})
        );
        assert_eq!(
            contents[2]["parts"],
            json!([
                {"functionResponse": {"id": "t1", "name": "Read", "response": {"output": "pineapple"}}},
                {"functionResponse": {"id": "t2", "name": "Read", "response": {"error": "missing"}}},
            ])
        );
    }

    #[test]
    fn a_kept_signature_goes_back_on_the_part_it_came_with() {
        let body = body(&request(
            "gemini-3.8-flash-high",
            vec![
                Message::user("Read a"),
                Message::assistant(vec![
                    signed("", Some("sig-1")),
                    Block::ToolUse { id: "t1".into(), name: "Read".into(), input: json!({}) },
                ]),
                Message::user("go on"),
            ],
        ));
        let call = &body["request"]["contents"][1]["parts"][0];
        assert_eq!(call["thoughtSignature"], "sig-1");
        assert_eq!(body["request"]["contents"][1]["parts"].as_array().map(Vec::len), Some(1));
    }

    #[test]
    fn reasoning_from_another_provider_is_left_out() {
        let foreign = Block::Thinking { text: "elsewhere".into(), signature: Some("s".into()), origin: None };
        let body = body(&request(
            "gemini-3.8-flash-high",
            vec![Message::user("hi"), Message::assistant(vec![foreign, Block::Text("hello".into())])],
        ));
        assert_eq!(body["request"]["contents"][1]["parts"], json!([{"text": "hello"}]));
        assert_eq!(body["request"]["contents"][2], json!({"role": "user", "parts": [{"text": "Continue."}]}));
    }

    #[test]
    fn the_effort_is_the_thinking_level_the_protocol_names() {
        let levels = [Effort::Low, Effort::Medium, Effort::High, Effort::XHigh, Effort::Max].map(level);
        assert_eq!(levels, ["low", "medium", "high", "high", "high"]);
    }

    #[test]
    fn a_tool_without_parameters_gets_a_placeholder_and_images_go_inline() {
        let mut asked = request(
            "gemini-3.8-flash-high",
            vec![Message {
                role: Role::User,
                blocks: vec![
                    Block::Image(domain::Image { media_type: "image/png".into(), data: "aGk=".into() }),
                    Block::Text("Color?".into()),
                ],
            }],
        );
        asked.tools[0].schema = json!({"type": "object"});
        let body = body(&asked);
        let schema = &body["request"]["tools"][0]["functionDeclarations"][0]["parametersJsonSchema"];
        assert_eq!(schema["properties"]["_placeholder"]["type"], "boolean");
        assert_eq!(
            body["request"]["contents"][0]["parts"],
            json!([{"inlineData": {"mimeType": "image/png", "data": "aGk="}}, {"text": "Color?"}])
        );
    }
}
