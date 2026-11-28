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

fn shaped(ids: &Ids, run: &Run, span: &Span, capture: Option<Capture<'_>>) -> Value {
    let mut attributes = attributes(run, span);
    attributes.extend(capture.map(|capture| capture.attributes(run, span)).unwrap_or_default());
    let mut shaped = json!({
        "traceId": ids.trace,
        "spanId": ids.span(span.id).unwrap_or_default(),
        "name": genai::name(run, span),
        "kind": if span.kind == SpanKind::Llm { CLIENT } else { INTERNAL },
        "startTimeUnixNano": nanos(run.started_at_ms + span.start_ms),
        "endTimeUnixNano": nanos(run.started_at_ms + span.end_ms),
        "attributes": attributes,
    });
    let parent = if span.parent == 0 && run.attribution.item.is_some() { WORKFLOW_SPAN } else { span.parent };
    if let Some(parent) = ids.span(parent).filter(|_| parent != 0) {
        shaped["parentSpanId"] = json!(parent);
    }
    let failed = span.failed || (span.kind == SpanKind::Run && run.status == Status::Failed);
    if failed {
        shaped["status"] = json!({"code": ERROR});
    }
    shaped
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

fn spans_of(run: &Run, capture: Option<Capture<'_>>) -> Vec<Value> {
    let ids = Ids::of(run);
    let mut spans: Vec<Value> = workflow(&ids, run).into_iter().collect();
    spans.extend(run.spans.iter().map(|span| shaped(&ids, run, span, capture)));
    spans
}

pub fn request(runs: &[Run], version: &str) -> Value {
    request_with(runs, version, None)
}

pub fn request_with(runs: &[Run], version: &str, capture: Option<Capture<'_>>) -> Value {
    let spans: Vec<Value> =
        runs.iter().filter(|run| run.status != Status::Running).flat_map(|run| spans_of(run, capture)).collect();
    json!({
        "resourceSpans": [{
            "resource": {"attributes": [text("service.name", brand::COMMAND), text("service.version", version)]},
            "scopeSpans": [{"scope": {"name": brand::COMMAND, "version": version}, "spans": spans}]
        }]
    })
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod conventions_tests;
