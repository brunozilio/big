use std::collections::HashMap;

use crate::trace::runs::{Run, SpanKind};

pub const WORKFLOW_SPAN: u32 = u32::MAX;
const CALL: &str = "call";

fn fnv(seed: u64, parts: &[&[u8]]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64 ^ seed;
    for part in parts {
        for byte in part.iter().chain(&[0xff]) {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
    hash
}

fn hex16(id: u64) -> String {
    format!("{:016x}", if id == 0 { 1 } else { id })
}

pub fn trace_id(session: &str, run: usize) -> String {
    let parts: [&[u8]; 2] = [session.as_bytes(), &run.to_le_bytes()];
    format!("{:016x}{:016x}", fnv(1, &parts), fnv(2, &parts))
}

pub fn span_id(session: &str, run: usize, span: u32) -> String {
    hex16(fnv(3, &[session.as_bytes(), &run.to_le_bytes(), &span.to_le_bytes()]))
}

pub fn call_span_id(trace: &str, call: &str) -> String {
    hex16(fnv(4, &[trace.as_bytes(), call.as_bytes()]))
}

pub fn valid_trace(trace: &str) -> bool {
    trace.len() == 32 && trace.bytes().all(|byte| byte.is_ascii_hexdigit()) && trace.bytes().any(|byte| byte != b'0')
}

pub fn traceparent(trace: &str, call: &str) -> Option<String> {
    let trace = trace.to_ascii_lowercase();
    (valid_trace(&trace) && !call.is_empty()).then(|| format!("00-{trace}-{}-01", call_span_id(&trace, call)))
}

#[derive(Debug)]
pub struct Ids {
    pub trace: String,
    pub spans: HashMap<u32, String>,
}

impl Ids {
    pub fn of(run: &Run) -> Ids {
        let given = run.trace.as_deref().map(str::to_ascii_lowercase).filter(|trace| valid_trace(trace));
        let trace = given.clone().unwrap_or_else(|| trace_id(&run.session, run.index));
        let mut spans: HashMap<u32, String> = run
            .spans
            .iter()
            .map(|span| {
                let call = span.attr(CALL).filter(|call| span.kind == SpanKind::Tool && !call.is_empty());
                let id = match (&given, call) {
                    (Some(trace), Some(call)) => call_span_id(trace, call),
                    _ => span_id(&run.session, run.index, span.id),
                };
                (span.id, id)
            })
            .collect();
        spans.insert(WORKFLOW_SPAN, span_id(&run.session, run.index, WORKFLOW_SPAN));
        Ids { trace, spans }
    }

    pub fn span(&self, id: u32) -> Option<&str> {
        self.spans.get(&id).map(String::as_str)
    }
}
