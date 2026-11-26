use std::time::Duration;

use serde_json::{Value, json};

const NAME: &str = "ggml-org/Qwen3-0.6B-GGUF/Qwen3-0.6B-Q4_0.gguf";
const URL: &str = "https://huggingface.co/ggml-org/Qwen3-0.6B-GGUF/resolve/main/Qwen3-0.6B-Q4_0.gguf";

fn arrived() {
    let request = local_models::Request { name: NAME.into(), url: URL.into(), headers: Vec::new(), bytes: None };
    local_models::download(request).unwrap_or_else(|error| panic!("{error}"));
    for _ in 0..600 {
        let transfer = local_models::transfers().into_iter().find(|transfer| transfer.id == NAME);
        if let Some(error) = transfer.as_ref().and_then(|transfer| transfer.error.clone()) {
            panic!("{error}");
        }
        if transfer.is_none_or(|transfer| !transfer.running) {
            return;
        }
        std::thread::sleep(Duration::from_secs(1));
    }
    panic!("the download took too long");
}

fn asked(body: &Value) -> Value {
    let base = local_models::endpoint().unwrap_or_else(|error| panic!("{error}"));
    let runtime = tokio::runtime::Runtime::new().unwrap_or_else(|error| panic!("{error}"));
    runtime.block_on(async {
        let client = reqwest::Client::new();
        let answer = client.post(format!("{base}/chat/completions")).json(body).send().await;
        answer.unwrap_or_else(|error| panic!("{error}")).json::<Value>().await.unwrap_or_else(|error| panic!("{error}"))
    })
}

#[test]
#[ignore = "downloads a small model and runs it"]
fn a_small_model_answers_and_calls_a_tool() {
    arrived();
    let formats = json!([{ "when": "<tool_call>", "think": { "open": "<think>", "close": "</think>" },
        "call": { "open": "<tool_call>", "close": "</tool_call>", "format": "json" } }]);
    let tools = json!([{ "type": "function", "function": { "name": "weather", "description": "Weather for a city",
        "parameters": { "type": "object", "properties": { "city": { "type": "string" } }, "required": ["city"] } } }]);
    let body = json!({ "model": NAME, "stream": false, "max_tokens": 600, "temperature": 0.0, "tools": tools,
        "messages": [{ "role": "user", "content": "What is the weather in Lisbon? Use the tool." }],
        "local": { "context": 4096, "formats": formats } });
    let first = asked(&body);
    println!("{first:#}");
    let call = &first["choices"][0]["message"]["tool_calls"][0];
    assert_eq!(call["function"]["name"], "weather");
    let mut again = body.clone();
    if let Some(messages) = again["messages"].as_array_mut() {
        messages.push(json!({ "role": "assistant", "content": "", "tool_calls": [call] }));
        messages.push(json!({ "role": "tool", "tool_call_id": call["id"], "content": "Sunny, 24C" }));
    }
    let second = asked(&again);
    println!("{second:#}");
    local_models::unload();
    assert!(second["usage"]["prompt_tokens_details"]["cached_tokens"].as_u64().unwrap_or_default() > 10);
    assert!(second["choices"][0]["message"]["content"].as_str().unwrap_or_default().contains("24"));
}
