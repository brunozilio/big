mod content;
mod genai;
mod ids;

use serde_json::{Value, json};

pub use self::content::Capture;
pub use self::genai::{CHAT, EXECUTE_TOOL, INVOKE_AGENT, INVOKE_WORKFLOW};
use self::ids::WORKFLOW_SPAN;
pub use self::ids::{Ids, call_span_id, span_id, trace_id, traceparent, valid_trace};
use super::runs::{Run, Span, SpanKind, Status};

const INTERNAL: u8 = 1;
const CLIENT: u8 = 3;
const ERROR: u8 = 2;

fn text(key: &str, value: &str) -> Value {
    json!({"key": key, "value": {"stringValue": value}})
}

fn whole(key: &str, value: u64) -> Value {
    json!({"key": key, "value": {"intValue": value.to_string()}})
}

fn real(key: &str, value: f64) -> Value {
    json!({"key": key, "value": {"doubleValue": value}})
}

fn own(key: &str) -> String {
    format!("{}.{}", brand::COMMAND, key.replace(' ', "_"))
}

fn nanos(ms: u64) -> String {
    (u128::from(ms) * 1_000_000).to_string()
}

fn kind_name(kind: SpanKind) -> &'static str {
    match kind {
        SpanKind::Run => "run",
        SpanKind::Llm => "llm",
        SpanKind::Tool => "tool",
        SpanKind::Agent => "agent",
        SpanKind::Task => "task",
        SpanKind::Eval => "eval",
        SpanKind::Wait => "wait",
        SpanKind::Event => "event",
    }
}

fn status_name(status: Status) -> &'static str {
    match status {
        Status::Ok => "ok",
        Status::Failed => "failed",
        Status::Cancelled => "cancelled",
        Status::Running => "running",
    }
}

fn attributes(run: &Run, span: &Span) -> Vec<Value> {
    let mut attributes = vec![
        text(&own("span.kind"), kind_name(span.kind)),
        text(&own("span.name"), &span.name),
        whole(&own("duration_ms"), span.duration()),
    ];
    if span.kind == SpanKind::Run {
        attributes.extend([
            text("session.id", &run.session),
            whole(&own("run.index"), run.index as u64),
            text(&own("run.status"), status_name(run.status)),
            whole(&own("tokens"), run.tokens),
            real(&own("cost_usd"), run.cost_usd),
            whole(&own("waiting_ms"), run.waiting_ms),
        ]);
    }
    if let Some(usage) = span.usage.as_ref().filter(|_| span.kind == SpanKind::Llm) {
        attributes.push(real(&own("cost_usd"), usage.cost_usd));
    }
    attributes.extend(genai::attributes(run, span));
    attributes.extend(span.attrs.iter().map(|(key, value)| text(&own(key), value)));
    attributes
}

#[derive(Debug, Clone, Copy)]
pub struct Export<'a> {
    pub capture: Option<Capture<'a>>,
    pub usage: bool,
    pub revision: &'a str,
}

fn usage_key(key: &str) -> bool {
    key.starts_with("gen_ai.usage.") || key.starts_with(&own("usage.")) || key == own("cost_usd") || key == own("tokens")
}

fn captured(export: &Export<'_>, run: &Run, span: &Span) -> Option<Vec<Value>> {
    let Some(capture) = export.capture else { return Some(Vec::new()) };
    let redacted = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| capture.attributes(run, span)));
    redacted.map_err(|_| crate::trace::capture::drop_one()).ok()
}

fn shaped(ids: &Ids, run: &Run, span: &Span, export: &Export<'_>) -> Option<Value> {
    let mut attributes = attributes(run, span);
    if !export.usage {
        attributes.retain(|attribute| !attribute["key"].as_str().is_some_and(usage_key));
    }
    attributes.extend(captured(export, run, span)?);
    let mut shaped = json!({
        "traceId": ids.trace,
        "spanId": ids.span(span.id).unwrap_or_default(),
        "name": genai::name(run, span),
        "kind": if span.kind == SpanKind::Llm { CLIENT } else { INTERNAL },
        "startTimeUnixNano": nanos(run.started_at_ms + span.start_ms),
        "endTimeUnixNano": nanos(run.started_at_ms + span.end_ms),
        "attributes": attributes,
    });
    let parent = if span.parent == 0 && genai::item(run).is_some() { WORKFLOW_SPAN } else { span.parent };
    if let Some(parent) = ids.span(parent).filter(|_| parent != 0) {
        shaped["parentSpanId"] = json!(parent);
    }
    let failed = span.failed || (span.kind == SpanKind::Run && run.status == Status::Failed);
    if failed {
        shaped["status"] = json!({"code": ERROR});
    }
    Some(shaped)
}

fn workflow(ids: &Ids, run: &Run) -> Option<Value> {
    let (name, attributes) = genai::workflow(run)?;
    let root = run.spans.first()?;
    let mut shaped = json!({
        "traceId": ids.trace,
        "spanId": ids.span(WORKFLOW_SPAN).unwrap_or_default(),
        "name": name,
        "kind": INTERNAL,
        "startTimeUnixNano": nanos(run.started_at_ms + root.start_ms),
        "endTimeUnixNano": nanos(run.started_at_ms + root.end_ms),
        "attributes": attributes,
    });
    if run.status == Status::Failed {
        shaped["status"] = json!({"code": ERROR});
    }
    Some(shaped)
}

fn spans_of(run: &Run, export: &Export<'_>) -> Vec<Value> {
    let ids = Ids::of(run);
    let mut spans: Vec<Value> = workflow(&ids, run).into_iter().collect();
    spans.extend(run.spans.iter().filter_map(|span| shaped(&ids, run, span, export)));
    spans
}

pub fn request(runs: &[Run], version: &str) -> Value {
    request_with(runs, version, None)
}

pub fn request_with(runs: &[Run], version: &str, capture: Option<Capture<'_>>) -> Value {
    exported(runs, version, &Export { capture, usage: true, revision: "" })
}

pub fn exported(runs: &[Run], version: &str, export: &Export<'_>) -> Value {
    let spans: Vec<Value> =
        runs.iter().filter(|run| run.status != Status::Running).flat_map(|run| spans_of(run, export)).collect();
    let mut resource = vec![text("service.name", brand::COMMAND), text("service.version", version)];
    if !export.revision.is_empty() {
        resource.push(text(&own("policy_revision"), export.revision));
    }
    json!({
        "resourceSpans": [{
            "resource": {"attributes": resource},
            "scopeSpans": [{"scope": {"name": brand::COMMAND, "version": version}, "spans": spans}]
        }]
    })
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod conventions_tests;

#[cfg(test)]
mod content_tests;
