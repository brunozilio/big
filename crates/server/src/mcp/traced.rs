use domain::mcp::McpEndpoint;
use serde_json::Value;

pub fn answer(endpoint: &dyn McpEndpoint, message: &Value) -> Option<Value> {
    let traceparent = domain::mcp::traceparent(message).unwrap_or_default();
    let span = tracing::info_span!("mcp.request", traceparent);
    span.in_scope(|| super::answer(endpoint, message))
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use serde_json::json;
    use tracing::field::{Field, Visit};
    use tracing::span::{Attributes, Id, Record};
    use tracing::{Event, Metadata, Subscriber};

    use super::*;

    #[derive(Default, Clone)]
    struct Spans(Arc<Mutex<Vec<(String, String)>>>);

    struct Seen<'a>(&'a mut String);

    impl Visit for Seen<'_> {
        fn record_str(&mut self, field: &Field, value: &str) {
            if field.name() == "traceparent" {
                value.clone_into(self.0);
            }
        }

        fn record_debug(&mut self, _field: &Field, _value: &dyn std::fmt::Debug) {}
    }

    impl Subscriber for Spans {
        fn enabled(&self, _metadata: &Metadata<'_>) -> bool {
            true
        }

        fn new_span(&self, span: &Attributes<'_>) -> Id {
            let mut given = String::new();
            span.record(&mut Seen(&mut given));
            let mut seen = self.0.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
            seen.push((span.metadata().name().to_owned(), given));
            Id::from_u64(seen.len() as u64)
        }

        fn record(&self, _span: &Id, _values: &Record<'_>) {}
        fn record_follows_from(&self, _span: &Id, _follows: &Id) {}
        fn event(&self, _event: &Event<'_>) {}
        fn enter(&self, _span: &Id) {}
        fn exit(&self, _span: &Id) {}
    }

    struct Echo;

    impl McpEndpoint for Echo {
        fn handle(&self, message: &Value) -> Option<Value> {
            message.get("id").map(|id| json!({ "jsonrpc": "2.0", "id": id, "result": {} }))
        }
    }

    #[test]
    fn the_caller_s_traceparent_is_kept_on_the_request_span_and_a_broken_one_is_not() {
        let spans = Spans::default();
        let given = "00-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-01";
        tracing::subscriber::with_default(spans.clone(), || {
            let call = json!({ "id": 1, "method": "tools/call", "params": { "_meta": { "traceparent": given } } });
            assert!(answer(&Echo, &call).is_some());
            let broken =
                json!({ "id": 2, "method": "tools/call", "params": { "_meta": { "traceparent": "00-x-y-01" } } });
            assert!(answer(&Echo, &broken).is_some());
        });
        let seen = spans.0.lock().map(|seen| seen.clone()).unwrap_or_default();
        let request = "mcp.request".to_owned();
        assert_eq!(seen, [(request.clone(), given.to_owned()), (request, String::new())]);
    }
}
