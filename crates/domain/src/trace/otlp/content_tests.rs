use serde_json::Value;

use super::conventions_tests::{execution, named, said, spans};
use super::*;
use crate::redact::{Known, Redactor, layer};
use crate::trace::build;
use crate::trace::capture::Keys;
use crate::trace::runs::{Run, runs};

const PERSONAL: &str = include_str!("../../../../../content/safety/personal.json");
const EMAILS: [&str; 4] = ["ana.souza@example.com", "bob@example.org", "carla@example.net", "dan@example.com"];

fn traced() -> Vec<Run> {
    runs("s-9", &build(execution()))
}

fn personal() -> Redactor {
    Redactor::compiled(layer(PERSONAL).unwrap_or_else(|error| panic!("{error}"))).0
}

const ALL: Keys = Keys { usage: true, content: true, prompts: true };

fn captured(redactor: &Redactor, known: &Known, limit: usize) -> Vec<Value> {
    spans(&request_with(&traced(), "1", Some(Capture { redactor, known, limit, keys: ALL })))
}

fn strings(value: &Value) -> Vec<String> {
    match value {
        Value::String(said) => vec![said.clone()],
        Value::Array(all) => all.iter().flat_map(strings).collect(),
        Value::Object(all) => all.values().flat_map(strings).collect(),
        _ => Vec::new(),
    }
}

fn first_chat(all: &[Value]) -> &Value {
    let found = all.iter().find(|span| span["name"] == "chat sonnet");
    found.unwrap_or_else(|| panic!("a chat span"))
}

#[test]
fn content_stays_home_unless_capture_is_on() {
    let quiet = request(&traced(), "1").to_string();
    for content in ["gen_ai.input.messages", "gen_ai.output.messages", "gen_ai.tool.call.arguments", "banker"] {
        assert!(!quiet.contains(content), "{content} left without capture");
    }
    assert!(EMAILS.iter().all(|email| !quiet.contains(email)));
}

#[test]
fn personal_data_leaves_redacted_from_the_prompt_the_model_calls_and_the_tools() {
    let all = captured(&personal(), &Known::default(), 4_096);
    let sent = all.iter().map(Value::to_string).collect::<String>();
    for email in EMAILS {
        assert!(!sent.contains(email), "{email} left the machine");
    }
    let lead = named(&all, "invoke_agent dev");
    let asked: Value = serde_json::from_str(&said(lead, "gen_ai.input.messages")).unwrap_or_default();
    assert_eq!(
        (asked[0]["role"].as_str(), asked[0]["parts"][0]["content"].as_str()),
        (Some("user"), Some("fix the totals, ask [redacted]"))
    );

    let chat = first_chat(&all);
    let chat_in = said(chat, "gen_ai.input.messages");
    assert!(chat_in.contains("fix the totals, ask [redacted]"), "{chat_in}");
    let chat_out = said(chat, "gen_ai.output.messages");
    assert!(chat_out.contains("I will mail [redacted]"), "{chat_out}");

    let mcp = named(&all, "execute_tool mcp__docs__search");
    let arguments = said(mcp, "gen_ai.tool.call.arguments");
    assert!(arguments.contains("rounding") && arguments.contains("[redacted]"), "{arguments}");
    let result = said(mcp, "gen_ai.tool.call.result");
    assert_eq!(result, "use banker's rounding, asked [redacted] with supersecretvalue1");
}

#[test]
fn a_secret_known_from_the_project_is_redacted_too() {
    let known = Known::from_env_files(["API_TOKEN=supersecretvalue1"]);
    let all = captured(&personal(), &known, 4_096);
    let result = said(named(&all, "execute_tool mcp__docs__search"), "gen_ai.tool.call.result");
    assert_eq!(result, "use banker's rounding, asked [redacted] with [redacted]");
}

#[test]
fn the_limit_caps_each_attribute_as_a_whole_keeping_the_latest_text() {
    let limit = 12;
    let all = captured(&personal(), &Known::default(), limit);
    for span in &all {
        for key in ["gen_ai.input.messages", "gen_ai.output.messages", "gen_ai.tool.call.arguments"] {
            let kept = said(span, key);
            let parsed: Value = serde_json::from_str(&kept).unwrap_or(Value::String(kept.clone()));
            let total: usize = strings(&parsed).iter().map(|one| one.trim_end_matches(" …").chars().count()).sum();
            assert!(total <= limit, "{key} of {} keeps {total} characters: {kept}", span["name"]);
        }
    }
    let chat_in = said(first_chat(&all), "gen_ai.input.messages");
    assert!(chat_in.contains(" …"), "{chat_in}");
}

#[test]
fn without_content_capture_no_tool_arguments_or_answers_leave() {
    let redactor = personal();
    let known = Known::default();
    let keys = Keys { content: false, ..ALL };
    let export = Export { capture: Some(Capture { redactor: &redactor, known: &known, limit: 4_096, keys }), usage: true, revision: "r1" };
    let sent = exported(&traced(), "1", &export).to_string();
    for content in ["gen_ai.tool.call.arguments", "gen_ai.tool.call.result", "gen_ai.output.messages", "rounding"] {
        assert!(!sent.contains(content), "{content} left without content capture");
    }
    assert!(sent.contains("gen_ai.input.messages"), "prompts are still captured");
}

#[test]
fn each_export_names_its_policy_and_usage_leaves_only_when_captured() {
    let export = Export { capture: None, usage: false, revision: "abc123" };
    let sent = exported(&traced(), "1", &export);
    let resource = sent["resourceSpans"][0]["resource"]["attributes"].to_string();
    assert!(resource.contains("policy_revision") && resource.contains("abc123"), "{resource}");
    let text = sent.to_string();
    assert!(!text.contains("gen_ai.usage.") && !text.contains("cost_usd"), "{text}");
    assert!(request(&traced(), "1").to_string().contains("gen_ai.usage.input_tokens"));
}
