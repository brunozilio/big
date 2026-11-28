use std::collections::HashMap;

use serde_json::Value;

use super::{text, whole};
use crate::trace::runs::{Run, Span, SpanKind, Usage};

pub const INVOKE_AGENT: &str = "invoke_agent";
pub const INVOKE_WORKFLOW: &str = "invoke_workflow";
pub const EXECUTE_TOOL: &str = "execute_tool";
pub const CHAT: &str = "chat";
const MAIN_AGENT: &str = "main";
const MCP_TOOL: &str = "mcp__";

pub fn operation(span: &Span) -> Option<&'static str> {
    match span.kind {
        SpanKind::Run | SpanKind::Agent => Some(INVOKE_AGENT),
        SpanKind::Tool => Some(EXECUTE_TOOL),
        SpanKind::Llm => Some(CHAT),
        _ => None,
    }
}

pub fn agent_name<'a>(run: &'a Run, span: &'a Span) -> &'a str {
    match span.kind {
        SpanKind::Run => run.agent.as_deref().filter(|name| !name.is_empty()).unwrap_or(MAIN_AGENT),
        _ => span.attr("agent").unwrap_or_else(|| span.name.split(" · ").next().unwrap_or(&span.name)),
    }
}

pub fn tool_name(span: &Span) -> &str {
    span.attr("tool").unwrap_or(&span.name)
}

pub fn name(run: &Run, span: &Span) -> String {
    match span.kind {
        SpanKind::Llm => format!("{CHAT} {}", span.usage.as_ref().map_or(span.name.as_str(), |usage| &usage.model)),
        SpanKind::Tool => format!("{EXECUTE_TOOL} {}", tool_name(span)),
        SpanKind::Run | SpanKind::Agent => format!("{INVOKE_AGENT} {}", agent_name(run, span)),
        _ => span.name.clone(),
    }
}

fn below(run: &Run, top: u32) -> Usage {
    let parents: HashMap<u32, u32> = run.spans.iter().map(|span| (span.id, span.parent)).collect();
    let within = |mut id: u32| {
        for _ in 0..=run.spans.len() {
            if id == top {
                return true;
            }
            match parents.get(&id) {
                Some(&parent) if parent != 0 => id = parent,
                _ => return false,
            }
        }
        false
    };
    let spent = run.spans.iter().filter(|span| within(span.id)).filter_map(|span| span.usage.as_ref());
    spent.fold(Usage::default(), |total, usage| Usage {
        input: total.input + usage.input,
        output: total.output + usage.output,
        cache_read: total.cache_read + usage.cache_read,
        cache_write: total.cache_write + usage.cache_write,
        ..total
    })
}

fn tokens(usage: &Usage) -> [Value; 4] {
    [
        whole("gen_ai.usage.input_tokens", usage.input),
        whole("gen_ai.usage.output_tokens", usage.output),
        whole("gen_ai.usage.cache_read_input_tokens", usage.cache_read),
        whole("gen_ai.usage.cache_creation_input_tokens", usage.cache_write),
    ]
}

fn provider(run: &Run) -> Option<&str> {
    run.spans.first().and_then(|root| root.attr("provider")).filter(|name| !name.is_empty())
}

pub fn attributes(run: &Run, span: &Span) -> Vec<Value> {
    let mut attributes: Vec<Value> = operation(span).map(|op| text("gen_ai.operation.name", op)).into_iter().collect();
    attributes.push(text("gen_ai.conversation.id", &run.session));
    if let Some(name) = provider(run) {
        attributes.push(text("gen_ai.provider.name", name));
    }
    match span.kind {
        SpanKind::Run | SpanKind::Agent => {
            attributes.push(text("gen_ai.agent.name", agent_name(run, span)));
            if span.kind == SpanKind::Run {
                attributes.push(text("gen_ai.request.model", &run.model));
            }
            attributes.extend(tokens(&below(run, span.id)));
        }
        SpanKind::Tool => {
            let name = tool_name(span);
            let kind = if name.starts_with(MCP_TOOL) { "extension" } else { "function" };
            attributes.extend([text("gen_ai.tool.name", name), text("gen_ai.tool.type", kind)]);
            if let Some(call) = span.attr("call").filter(|call| !call.is_empty()) {
                attributes.push(text("gen_ai.tool.call.id", call));
            }
        }
        SpanKind::Llm => {
            if let Some(usage) = &span.usage {
                attributes.push(text("gen_ai.request.model", &usage.model));
                attributes.extend(tokens(usage));
            }
            if let Some(stop) = &span.stop {
                attributes.push(text("gen_ai.response.finish_reasons", stop));
            }
        }
        _ => {}
    }
    attributes
}

pub fn workflow(run: &Run) -> Option<(String, Vec<Value>)> {
    let item = run.attribution.item.as_deref().filter(|item| !item.is_empty())?;
    let step = run.attribution.step.as_deref().unwrap_or_default();
    let name = if step.is_empty() { INVOKE_WORKFLOW.to_owned() } else { format!("{INVOKE_WORKFLOW} {step}") };
    let mut attributes = vec![
        text("gen_ai.operation.name", INVOKE_WORKFLOW),
        text("gen_ai.conversation.id", &run.session),
        text(&super::own("item"), item),
        text(&super::own("origin"), run.attribution.origin.word()),
    ];
    if !step.is_empty() {
        attributes.push(text(&super::own("step"), step));
    }
    attributes.extend(tokens(&below(run, 1)));
    Some((name, attributes))
}
