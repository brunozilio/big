use domain::{Effort, ModelProvider};
use serde_json::{Value, json};

use crate::model::{BlockingTurn, CancellationToken, ToolSpec};
use crate::providers::testing::{client, request};

const ANTIGRAVITY: ModelProvider = ModelProvider::new("antigravity-subscription");

const ANSWER: &str = "data: {\"response\":{\"candidates\":[{\"content\":{\"role\":\"model\",\"parts\":[{\"text\":\"hello\"}]},\"finishReason\":\"STOP\"}]}}\n\n";

fn sent(model: &str, effort: Effort, tools: bool) -> String {
    sent_at_most(model, effort, tools, 64000)
}

fn sent_at_most(model: &str, effort: Effort, tools: bool, most: u32) -> String {
    let (client, http) = client(ANTIGRAVITY, [(200, ANSWER.to_owned())]);
    let mut asked = request(&format!("antigravity-subscription:{model}"));
    asked.effort = effort;
    asked.max_tokens = most;
    if tools {
        asked.tools = vec![ToolSpec {
            name: "Read".into(),
            description: "Reads".into(),
            schema: json!({"type": "object", "properties": {"file_path": {"type": "string"}}}),
        }];
    }
    client.turn_blocking(&asked, &mut |_| {}, &CancellationToken::new()).unwrap_or_else(|e| panic!("{e}"));
    let mut body: Value = serde_json::from_str(&http.sent()[0].body).unwrap_or_else(|e| panic!("{e}"));
    let id = body["requestId"].as_str().unwrap_or_default().to_owned();
    assert!(id.starts_with("agent-") && uuid::Uuid::parse_str(&id["agent-".len()..]).is_ok(), "{id}");
    body["requestId"] = json!("agent-ID");
    body["request"]["sessionId"] = json!("SESSION");
    body.to_string()
}

const SENT: &[(&str, Effort, bool, &str)] = &[
    (
        "claude-opus-5-5-high",
        Effort::Medium,
        true,
        r#"{"model":"claude-opus-5-5-high","project":"account-1","request":{"contents":[{"parts":[{"text":"hello"}],"role":"user"}],"generationConfig":{"maxOutputTokens":64000,"thinkingConfig":{"include_thoughts":true,"thinking_budget":32768}},"sessionId":"SESSION","systemInstruction":{"parts":[{"text":"Help the user."}],"role":"user"},"toolConfig":{"functionCallingConfig":{"mode":"VALIDATED"}},"tools":[{"functionDeclarations":[{"description":"Reads","name":"Read","parametersJsonSchema":{"properties":{"file_path":{"type":"string"}},"type":"object"}}]}]},"requestId":"agent-ID","requestType":"agent","tools":null,"userAgent":"antigravity"}"#,
    ),
    (
        "claude-sonnet-5-5-thinking",
        Effort::Low,
        false,
        r#"{"model":"claude-sonnet-5-5-thinking","project":"account-1","request":{"contents":[{"parts":[{"text":"hello"}],"role":"user"}],"generationConfig":{"maxOutputTokens":64000,"thinkingConfig":{"include_thoughts":true,"thinking_budget":8192}},"sessionId":"SESSION","systemInstruction":{"parts":[{"text":"Help the user."}],"role":"user"}},"requestId":"agent-ID","requestType":"agent","tools":null,"userAgent":"antigravity"}"#,
    ),
    (
        "claude-sonnet-5-5",
        Effort::High,
        true,
        r#"{"model":"claude-sonnet-5-5","project":"account-1","request":{"contents":[{"parts":[{"text":"hello"}],"role":"user"}],"generationConfig":{"maxOutputTokens":64000},"sessionId":"SESSION","systemInstruction":{"parts":[{"text":"Help the user."}],"role":"user"},"toolConfig":{"functionCallingConfig":{"mode":"VALIDATED"}},"tools":[{"functionDeclarations":[{"description":"Reads","name":"Read","parametersJsonSchema":{"properties":{"file_path":{"type":"string"}},"type":"object"}}]}]},"requestId":"agent-ID","requestType":"agent","tools":null,"userAgent":"antigravity"}"#,
    ),
    (
        "gemini-3.8-flash-high",
        Effort::Medium,
        true,
        r#"{"model":"gemini-3.8-flash-high","project":"account-1","request":{"contents":[{"parts":[{"text":"hello"}],"role":"user"}],"generationConfig":{"maxOutputTokens":64000,"thinkingConfig":{"includeThoughts":true}},"sessionId":"SESSION","systemInstruction":{"parts":[{"text":"Help the user."}],"role":"user"},"tools":[{"functionDeclarations":[{"description":"Reads","name":"Read","parametersJsonSchema":{"properties":{"file_path":{"type":"string"}},"type":"object"}}]}]},"requestId":"agent-ID","requestType":"agent","tools":null,"userAgent":"antigravity"}"#,
    ),
    (
        "gemini-4-argon",
        Effort::Low,
        false,
        r#"{"model":"gemini-4-argon","project":"account-1","request":{"contents":[{"parts":[{"text":"hello"}],"role":"user"}],"generationConfig":{"maxOutputTokens":64000,"thinkingConfig":{"includeThoughts":true,"thinkingLevel":"low"}},"sessionId":"SESSION","systemInstruction":{"parts":[{"text":"Help the user."}],"role":"user"}},"requestId":"agent-ID","requestType":"agent","tools":null,"userAgent":"antigravity"}"#,
    ),
    (
        "gemini-4-argon",
        Effort::Max,
        false,
        r#"{"model":"gemini-4-argon","project":"account-1","request":{"contents":[{"parts":[{"text":"hello"}],"role":"user"}],"generationConfig":{"maxOutputTokens":64000,"thinkingConfig":{"includeThoughts":true,"thinkingLevel":"high"}},"sessionId":"SESSION","systemInstruction":{"parts":[{"text":"Help the user."}],"role":"user"}},"requestId":"agent-ID","requestType":"agent","tools":null,"userAgent":"antigravity"}"#,
    ),
    (
        "gemini-2.5-flash",
        Effort::Medium,
        false,
        r#"{"model":"gemini-2.5-flash","project":"account-1","request":{"contents":[{"parts":[{"text":"hello"}],"role":"user"}],"generationConfig":{"maxOutputTokens":64000,"thinkingConfig":{"includeThoughts":true,"thinkingBudget":12288}},"sessionId":"SESSION","systemInstruction":{"parts":[{"text":"Help the user."}],"role":"user"}},"requestId":"agent-ID","requestType":"agent","tools":null,"userAgent":"antigravity"}"#,
    ),
    (
        "gemini-2.5-pro",
        Effort::XHigh,
        false,
        r#"{"model":"gemini-2.5-pro","project":"account-1","request":{"contents":[{"parts":[{"text":"hello"}],"role":"user"}],"generationConfig":{"maxOutputTokens":64000,"thinkingConfig":{"includeThoughts":true,"thinkingBudget":32768}},"sessionId":"SESSION","systemInstruction":{"parts":[{"text":"Help the user."}],"role":"user"}},"requestId":"agent-ID","requestType":"agent","tools":null,"userAgent":"antigravity"}"#,
    ),
    (
        "gemini-2.0-lite-minimal",
        Effort::High,
        false,
        r#"{"model":"gemini-2.0-lite-minimal","project":"account-1","request":{"contents":[{"parts":[{"text":"hello"}],"role":"user"}],"generationConfig":{"maxOutputTokens":64000,"thinkingConfig":{"includeThoughts":true,"thinkingBudget":4096}},"sessionId":"SESSION","systemInstruction":{"parts":[{"text":"Help the user."}],"role":"user"}},"requestId":"agent-ID","requestType":"agent","tools":null,"userAgent":"antigravity"}"#,
    ),
    (
        "gpt-oss-120b-medium",
        Effort::High,
        true,
        r#"{"model":"gpt-oss-120b-medium","project":"account-1","request":{"contents":[{"parts":[{"text":"hello"}],"role":"user"}],"generationConfig":{"maxOutputTokens":64000},"sessionId":"SESSION","systemInstruction":{"parts":[{"text":"Help the user."}],"role":"user"},"tools":[{"functionDeclarations":[{"description":"Reads","name":"Read","parametersJsonSchema":{"properties":{"file_path":{"type":"string"}},"type":"object"}}]}]},"requestId":"agent-ID","requestType":"agent","tools":null,"userAgent":"antigravity"}"#,
    ),
];

#[test]
fn the_body_on_the_wire_is_the_one_sent_before_the_rules_moved_to_the_provider() {
    for (model, effort, tools, wanted) in SENT {
        assert_eq!(sent(model, *effort, *tools), *wanted, "{model} {effort:?}");
    }
}

#[test]
fn a_small_output_grows_past_the_thinking_budget() {
    let sent = sent_at_most("claude-opus-5-5-high", Effort::Low, false, 8192);
    assert!(sent.contains(r#""generationConfig":{"maxOutputTokens":40960,"thinkingConfig":{"include_thoughts":true,"thinking_budget":32768}}"#), "{sent}");
    let sent = sent_at_most("gemini-4-argon", Effort::Low, false, 8192);
    assert!(sent.contains(r#""generationConfig":{"maxOutputTokens":8192,"thinkingConfig":{"includeThoughts":true,"thinkingLevel":"low"}}"#), "{sent}");
}
