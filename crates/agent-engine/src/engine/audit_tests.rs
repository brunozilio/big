use super::*;

type Pairs = Vec<(String, String)>;

#[derive(Default, Clone)]
struct Heard(std::sync::Arc<Mutex<Vec<Pairs>>>);

struct Fields<'a>(&'a mut Pairs);

impl tracing::field::Visit for Fields<'_> {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        self.0.push((field.name().to_owned(), format!("{value:?}")));
    }
    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        self.0.push((field.name().to_owned(), value.to_owned()));
    }
}

impl tracing::Subscriber for Heard {
    fn enabled(&self, _: &tracing::Metadata<'_>) -> bool {
        true
    }
    fn new_span(&self, _: &tracing::span::Attributes<'_>) -> tracing::span::Id {
        tracing::span::Id::from_u64(1)
    }
    fn record(&self, _: &tracing::span::Id, _: &tracing::span::Record<'_>) {}
    fn record_follows_from(&self, _: &tracing::span::Id, _: &tracing::span::Id) {}
    fn event(&self, event: &tracing::Event<'_>) {
        let mut fields = Vec::new();
        event.record(&mut Fields(&mut fields));
        self.0.locked().push(fields);
    }
    fn enter(&self, _: &tracing::span::Id) {}
    fn exit(&self, _: &tracing::span::Id) {}
}

fn field<'a>(fields: &'a [(String, String)], name: &str) -> Option<&'a str> {
    fields.iter().find(|(key, _)| key == name).map(|(_, value)| value.as_str())
}

#[test]
fn a_fallback_is_audited_as_a_decision_with_the_models_it_skipped() {
    use domain::data_policy::{Rerouted, Skipped};
    let heard = Heard::default();
    let skipped = vec![Skipped { model: "a:us".into(), why: "region us is not eu".into() }];
    let decision = Rerouted { from: "a:x".into(), to: Some("b:eu".into()), failure: "overloaded".into(), skipped };
    tracing::subscriber::with_default(heard.clone(), || rest("s1", None, &EngineEvent::Rerouted(decision)));
    let events = heard.0.locked().clone();
    let [fields] = events.as_slice() else { panic!("{events:?}") };
    assert_eq!(field(fields, "message"), Some("agent.fallback.decision"));
    assert_eq!(field(fields, "session.id"), Some("s1"));
    assert_eq!(field(fields, "model.from"), Some("a:x"));
    assert_eq!(field(fields, "model.to"), Some("b:eu"));
    assert_eq!(field(fields, "error.kind"), Some("overloaded"));
    assert_eq!(field(fields, "model.skipped"), Some(r#"[{"model":"a:us","why":"region us is not eu"}]"#));
}

#[test]
fn a_preview_is_one_line_and_short() {
    assert_eq!(preview("one\n  two\tthree"), "one two three");
    let long = "a".repeat(PREVIEW_CHARS + 10);
    assert_eq!(preview(&long).chars().count(), PREVIEW_CHARS + 1);
}

#[test]
fn the_kind_of_an_event_is_its_variant_name() {
    assert_eq!(kind(&EngineEvent::Cancelled), "Cancelled");
    assert_eq!(kind(&EngineEvent::Checkpoint(3)), "Checkpoint");
    assert_eq!(kind(&EngineEvent::ToolFinished { id: "x".into() }), "ToolFinished");
}

fn heard_from(keys: Keys) -> Vec<Pairs> {
    let heard = Heard::default();
    let audit = Audit::capturing(keys);
    let input = Some(serde_json::json!({"query": "rounding"}));
    let call = ToolCall { id: "t1".into(), name: "Grep".into(), summary: String::new(), output: None, failed: false, input };
    tracing::subscriber::with_default(heard.clone(), || {
        audit.entry(&SessionId("s".into()), &Entry::Prompted { at_ms: 1, prompt: "fix it".into() });
        audit.event("s", None, 10, &EngineEvent::ToolStarted(call));
        audit.event("s", None, 20, &EngineEvent::ToolResult { id: "t1".into(), output: "found".into(), failed: false });
    });
    heard.0.locked().clone()
}

#[test]
fn without_content_capture_the_audit_keeps_no_tool_arguments_or_results() {
    let events = heard_from(Keys::default());
    let said: Vec<&(String, String)> = events.iter().flatten().collect();
    let names = ["gen_ai.tool.call.arguments", "gen_ai.tool.call.result", "prompt.text"];
    assert!(said.iter().all(|(key, _)| !names.contains(&key.as_str())), "{said:?}");
    assert!(said.iter().all(|(_, value)| !value.contains("rounding") && !value.contains("found")));
    assert!(events.iter().any(|fields| field(fields, "message") == Some("tool.start")));
    assert!(events.iter().any(|fields| field(fields, "message") == Some("tool.call")));
}

#[test]
fn with_content_and_prompt_capture_the_audit_keeps_what_it_did_before() {
    let events = heard_from(Keys { usage: true, content: true, prompts: true });
    let value = |name| events.iter().find_map(|fields| field(fields, name).map(str::to_owned));
    assert_eq!(value("gen_ai.tool.call.arguments").as_deref(), Some(r#"{"query":"rounding"}"#));
    assert_eq!(value("gen_ai.tool.call.result").as_deref(), Some("found"));
    assert_eq!(value("prompt.text").as_deref(), Some("fix it"));
}

#[test]
fn a_tool_is_closed_by_its_result_even_inside_a_subagent() {
    let audit = Audit::capturing(Keys::default());
    let call = ToolCall {
        id: "t1".into(),
        name: "Grep".into(),
        summary: String::new(),
        output: None,
        failed: false,
        input: None,
    };
    let inside = |event| EngineEvent::AgentEvent { index: 1, event: Box::new(event) };
    audit.event("s", None, 10, &inside(EngineEvent::ToolStarted(call)));
    assert_eq!(
        audit.tools.locked().get("t1").map(|open| (open.name.clone(), open.at_ms)),
        Some(("Grep".into(), 10))
    );
    audit.event(
        "s",
        None,
        40,
        &inside(EngineEvent::ToolResult { id: "t1".into(), output: "ok".into(), failed: false }),
    );
    assert!(audit.tools.locked().is_empty());
}
