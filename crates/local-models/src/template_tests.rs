use serde_json::json;

use super::*;

const QWEN: &str = include_str!("../tests/data/qwen.jinja");

fn rendered(messages: &Value, tools: &Value, extra: &Value) -> String {
    let rendering = Rendering { template: QWEN, messages, tools, extra, bos: "", eos: "<|im_end|>" };
    render(&rendering).unwrap_or_else(|error| panic!("{error}"))
}

#[test]
fn a_real_template_renders_tools_calls_and_results() {
    let messages = json!([
        { "role": "system", "content": "Be brief." },
        { "role": "user", "content": "Read a.rs" },
        { "role": "assistant", "content": null, "tool_calls": [
            { "id": "c1", "type": "function", "function": { "name": "read", "arguments": "{\"path\":\"a.rs\",\"limit\":3}" } }
        ] },
        { "role": "tool", "tool_call_id": "c1", "content": "fn main() {}" },
    ]);
    let tools =
        json!([{ "type": "function", "function": { "name": "read", "description": "Read", "parameters": {} } }]);
    let prompt = rendered(&messages, &tools, &json!({}));
    assert!(prompt.starts_with("<|im_start|>system\n"), "{prompt}");
    assert!(prompt.contains("Be brief."));
    assert!(prompt.contains("<tool_call>\n<function=read>\n"), "{prompt}");
    assert!(prompt.contains("<parameter=path>\na.rs\n</parameter>"), "{prompt}");
    assert!(prompt.contains("<parameter=limit>\n3\n</parameter>"), "{prompt}");
    assert!(prompt.contains("<tool_response>\nfn main() {}\n</tool_response>"), "{prompt}");
    assert!(prompt.ends_with("<|im_start|>assistant\n<think>\n"), "{prompt}");
}

#[test]
fn extra_values_reach_the_template() {
    let messages = json!([{ "role": "user", "content": [{ "type": "text", "text": "hi" }] }]);
    let prompt = rendered(&messages, &json!([]), &json!({ "enable_thinking": false }));
    assert!(prompt.contains("<|im_start|>user\nhi<|im_end|>"), "{prompt}");
    assert!(!prompt.ends_with("<think>\n"), "{prompt}");
}

#[test]
fn a_template_that_raises_says_why() {
    let empty = Value::Null;
    let rendering = Rendering {
        template: "{{ raise_exception('no') }}",
        messages: &empty,
        tools: &empty,
        extra: &empty,
        bos: "",
        eos: "",
    };
    assert!(render(&rendering).is_err_and(|error| error.contains("no")));
}
