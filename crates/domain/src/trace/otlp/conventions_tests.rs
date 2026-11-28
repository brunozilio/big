use std::collections::HashMap;

use serde_json::{Value, json};

use super::*;
use crate::billing::ModelUsage;
use crate::origin::{Attribution, Origin};
use crate::redact::{Redactor, layer};
use crate::trace::runs::runs;
use crate::trace::{Record, RunInfo, build};
use crate::{AgentStatus, EngineEvent, ToolCall};

const TRACE: &str = "4bf92f3577b34da6a3ce929d0e0e4736";
const PERSONAL: &str = include_str!("../../../../../content/safety/personal.json");

fn at(at_ms: u64, event: EngineEvent) -> Record {
    Record::Event { at_ms, event }
}

fn inside(event: EngineEvent) -> EngineEvent {
    EngineEvent::AgentEvent { index: 0, event: Box::new(event) }
}

fn spent(model: &str, input: u64, output: u64) -> EngineEvent {
    EngineEvent::Usage(ModelUsage {
        model: model.into(),
        input,
        output,
        cache_read: 5,
        estimated_cost_usd: Some(0.01),
        ..ModelUsage::default()
    })
}

fn started(id: &str, name: &str, input: Value) -> EngineEvent {
    EngineEvent::ToolStarted(ToolCall::new(id, name, "").with_input(input))
}

fn finished(id: &str, output: &str) -> EngineEvent {
    EngineEvent::ToolResult { id: id.into(), output: output.into(), failed: false }
}

fn execution() -> Vec<Record> {
    let info = RunInfo {
        model: "sonnet".into(),
        provider: "acme".into(),
        trace: Some(TRACE.into()),
        agent: Some("dev".into()),
        attribution: Attribution::of_step("item-7", "develop", Origin::Step),
        ..RunInfo::default()
    };
    let spawned = EngineEvent::AgentSpawned {
        index: 0,
        kind: "Explore".into(),
        summary: "map the code".into(),
        prompt: "find where totals are rounded".into(),
    };
    vec![
        Record::Prompted { at_ms: 1_000, prompt: "fix the totals, ask ana.souza@example.com".into() },
        Record::Opened { at_ms: 1_000, run: info },
        at(1_100, EngineEvent::TextDelta("Looking.".into())),
        at(1_200, spent("sonnet", 100, 10)),
        at(1_300, started("toolu_mcp", "mcp__docs__search", json!({"query": "rounding"}))),
        at(1_400, finished("toolu_mcp", "use banker's rounding")),
        at(1_500, spent("sonnet", 200, 20)),
        at(1_600, started("toolu_agent", "Agent", json!({"prompt": "find"}))),
        at(1_610, spawned),
        at(1_620, inside(EngineEvent::TextDelta("Searching.".into()))),
        at(1_700, inside(spent("haiku", 40, 4))),
        at(1_710, inside(started("toolu_read", "Read", json!({"path": "src/total.rs"})))),
        at(1_750, inside(finished("toolu_read", "fn total()"))),
        at(1_790, EngineEvent::AgentUpdate { index: 0, status: AgentStatus::Done { seconds: 1 }, summary: None }),
        at(1_800, finished("toolu_agent", "totals live in src/total.rs")),
        at(1_850, EngineEvent::TextDelta("Fixed.".into())),
        at(1_900, spent("sonnet", 300, 30)),
        at(2_000, EngineEvent::Completed { duration_ms: 1_000, cost_usd: Some(0.04) }),
    ]
}

fn spans(sent: &Value) -> Vec<Value> {
    sent["resourceSpans"][0]["scopeSpans"][0]["spans"].as_array().cloned().unwrap_or_default()
}

fn attribute<'a>(span: &'a Value, key: &str) -> Option<&'a Value> {
    let all = span["attributes"].as_array()?;
    all.iter().find(|one| one["key"] == key).map(|one| &one["value"])
}

fn said(span: &Value, key: &str) -> String {
    attribute(span, key).and_then(|value| value["stringValue"].as_str()).unwrap_or_default().to_owned()
}

fn count(span: &Value, key: &str) -> u64 {
    attribute(span, key).and_then(|value| value["intValue"].as_str()).and_then(|v| v.parse().ok()).unwrap_or(u64::MAX)
}

fn named<'a>(all: &'a [Value], name: &str) -> &'a Value {
    let found = all.iter().find(|span| span["name"] == name);
    found.unwrap_or_else(|| panic!("no span {name} in {:?}", all.iter().map(|s| &s["name"]).collect::<Vec<_>>()))
}

fn hex(text: &Value, length: usize) -> bool {
    text.as_str().is_some_and(|text| text.len() == length && text.bytes().all(|byte| byte.is_ascii_hexdigit()))
}

fn exported() -> Vec<Value> {
    spans(&request(&runs("s-9", &build(execution())), "1.0.0"))
}

#[test]
fn each_agent_tool_and_model_call_is_a_span_named_and_typed_by_its_gen_ai_operation() {
    let all = exported();
    let workflow = named(&all, "invoke_workflow develop");
    let lead = named(&all, "invoke_agent dev");
    let mcp = named(&all, "execute_tool mcp__docs__search");
    let task = named(&all, "execute_tool Agent");
    let explore = named(&all, "invoke_agent Explore");
    let read = named(&all, "execute_tool Read");
    let explore_chat = named(&all, "chat haiku");
    let chats: Vec<&Value> = all.iter().filter(|span| span["name"] == "chat sonnet").collect();
    assert_eq!(chats.len(), 3);
    let operations = [
        (workflow, "invoke_workflow"),
        (lead, "invoke_agent"),
        (mcp, "execute_tool"),
        (task, "execute_tool"),
        (explore, "invoke_agent"),
        (read, "execute_tool"),
        (explore_chat, "chat"),
    ];
    for (span, operation) in operations.into_iter().chain(chats.iter().map(|chat| (*chat, "chat"))) {
        assert_eq!(said(span, "gen_ai.operation.name"), operation, "{}", span["name"]);
        assert_eq!(said(span, "gen_ai.conversation.id"), "s-9");
    }
}

#[test]
fn the_sub_agent_sits_under_its_tool_and_the_lead_agent_under_the_workflow() {
    let all = exported();
    let workflow = named(&all, "invoke_workflow develop");
    let lead = named(&all, "invoke_agent dev");
    let mcp = named(&all, "execute_tool mcp__docs__search");
    let task = named(&all, "execute_tool Agent");
    let explore = named(&all, "invoke_agent Explore");
    let read = named(&all, "execute_tool Read");
    let explore_chat = named(&all, "chat haiku");
    let chats: Vec<&Value> = all.iter().filter(|span| span["name"] == "chat sonnet").collect();
    let parent = |span: &Value| span.get("parentSpanId").cloned().unwrap_or(Value::Null);
    assert_eq!(parent(workflow), Value::Null, "the workflow is the root");
    assert_eq!(parent(lead), workflow["spanId"]);
    assert!(chats.iter().all(|chat| parent(chat) == lead["spanId"]));
    assert_eq!(parent(mcp), lead["spanId"]);
    assert_eq!(parent(task), lead["spanId"]);
    assert_eq!(parent(explore), task["spanId"], "the sub-agent runs under the tool that started it");
    assert_eq!(parent(read), explore["spanId"]);
    assert_eq!(parent(explore_chat), explore["spanId"]);
}

#[test]
fn each_model_call_carries_its_tokens_and_each_agent_the_tokens_of_everything_below_it() {
    let all = exported();
    let workflow = named(&all, "invoke_workflow develop");
    let lead = named(&all, "invoke_agent dev");
    let mcp = named(&all, "execute_tool mcp__docs__search");
    let task = named(&all, "execute_tool Agent");
    let explore = named(&all, "invoke_agent Explore");
    let read = named(&all, "execute_tool Read");
    let explore_chat = named(&all, "chat haiku");
    let chats: Vec<&Value> = all.iter().filter(|span| span["name"] == "chat sonnet").collect();
    assert_eq!(
        (count(chats[0], "gen_ai.usage.input_tokens"), count(chats[0], "gen_ai.usage.output_tokens")),
        (100, 10)
    );
    assert_eq!(said(chats[0], "gen_ai.request.model"), "sonnet");
    assert_eq!(said(chats[0], "gen_ai.provider.name"), "acme");
    assert_eq!((count(explore, "gen_ai.usage.input_tokens"), count(explore, "gen_ai.usage.output_tokens")), (40, 4));
    assert_eq!((count(lead, "gen_ai.usage.input_tokens"), count(lead, "gen_ai.usage.output_tokens")), (640, 64));
    assert_eq!(count(lead, "gen_ai.usage.cache_read_input_tokens"), 20);
    assert_eq!(count(workflow, "gen_ai.usage.output_tokens"), 64);
    assert_eq!((said(lead, "gen_ai.agent.name"), said(explore, "gen_ai.agent.name")), ("dev".into(), "Explore".into()));
    assert_eq!((said(workflow, &own("item")), said(workflow, &own("step"))), ("item-7".into(), "develop".into()));

    assert_eq!(said(mcp, "gen_ai.tool.name"), "mcp__docs__search");
    assert_eq!(said(mcp, "gen_ai.tool.type"), "extension");
    assert_eq!(said(mcp, "gen_ai.tool.call.id"), "toolu_mcp");
    assert_eq!(said(read, "gen_ai.tool.type"), "function");

    assert_eq!(said(lead, &own("span.name")), "run", "the old span name and kind stay as attributes");
    assert_eq!(said(lead, &own("span.kind")), "run");
    assert_eq!(said(lead, "session.id"), "s-9");
    let _ = (task, explore_chat);
}

#[test]
fn the_payload_is_one_well_formed_otlp_trace_with_every_parent_in_it() {
    let all = exported();
    let ids: HashMap<&str, &Value> =
        all.iter().map(|span| (span["spanId"].as_str().unwrap_or_default(), span)).collect();
    assert_eq!(ids.len(), all.len(), "every span id is distinct");
    for span in &all {
        assert_eq!(span["traceId"], TRACE, "the whole execution is one trace, the one the run carried");
        assert!(hex(&span["spanId"], 16), "{span}");
        if let Some(up) = span.get("parentSpanId") {
            assert!(
                ids.contains_key(up.as_str().unwrap_or_default()),
                "the parent of {} is in the trace",
                span["name"]
            );
        }
        let (start, end) = (&span["startTimeUnixNano"], &span["endTimeUnixNano"]);
        let nanos = |value: &Value| value.as_str().and_then(|text| text.parse::<u128>().ok()).unwrap_or_default();
        assert!(nanos(start) > 0 && nanos(start) <= nanos(end), "{span}");
        assert!(matches!(span["kind"].as_u64(), Some(1 | 3)));
    }
}

#[test]
fn the_traceparent_sent_to_an_mcp_server_points_at_the_tool_span_that_called_it() {
    let all = exported();
    let mcp = named(&all, "execute_tool mcp__docs__search");
    let given = traceparent(TRACE, "toolu_mcp").unwrap_or_default();
    assert_eq!(given, format!("00-{TRACE}-{}-01", mcp["spanId"].as_str().unwrap_or_default()));
    let call = json!({ "method": "tools/call", "params": { "_meta": { "traceparent": given } } });
    assert_eq!(crate::mcp::traceparent(&call), Some(given.as_str()), "an MCP server reads the span it was called from");
}

#[test]
fn content_stays_home_unless_capture_is_on_and_then_it_leaves_redacted() {
    let traced = runs("s-9", &build(execution()));
    let quiet = request(&traced, "1").to_string();
    for content in ["gen_ai.input.messages", "gen_ai.tool.call.arguments", "ana.souza", "banker"] {
        assert!(!quiet.contains(content), "{content} left without capture");
    }

    let redactor = Redactor::compiled(layer(PERSONAL).unwrap_or_else(|error| panic!("{error}"))).0;
    let all = spans(&request_with(&traced, "1", Some(Capture { redactor: &redactor, limit: 64 })));
    let lead = named(&all, "invoke_agent dev");
    let asked: Value = serde_json::from_str(&said(lead, "gen_ai.input.messages")).unwrap_or_default();
    assert_eq!(asked[0]["role"], "user");
    assert_eq!(asked[0]["parts"][0]["content"], "fix the totals, ask [redacted]");
    let mcp = named(&all, "execute_tool mcp__docs__search");
    assert_eq!(said(mcp, "gen_ai.tool.call.result"), "use banker's rounding");
    assert!(said(mcp, "gen_ai.tool.call.arguments").contains("rounding"));
    let answer = said(named(&all, "chat haiku"), "gen_ai.output.messages");
    assert!(answer.contains("Searching."), "{answer}");
    let long = request_with(&traced, "1", Some(Capture { redactor: &redactor, limit: 8 })).to_string();
    assert!(long.contains(" …"), "a captured text is cut at the limit");
    assert!(!long.contains("ana.souza"));
}
