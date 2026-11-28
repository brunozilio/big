use std::collections::HashMap;
use std::sync::Mutex;

use domain::billing::ModelUsage;
use domain::trace::{ModelCall, RunInfo};
use domain::{AgentAsk, AgentStatus, AskKind, EngineEvent, SessionId, ToolCall};
use platform::sync::Locked;

use crate::sessions::Entry;

const PREVIEW_CHARS: usize = 500;
const MOST_OPEN_TOOLS: usize = 1024;

struct Open {
    name: String,
    at_ms: u64,
}

#[derive(Default)]
pub(super) struct Audit {
    tools: Mutex<HashMap<String, Open>>,
}

impl Audit {
    pub(super) fn entry(&self, session: &SessionId, entry: &Entry) {
        let session = session.0.as_str();
        match entry {
            Entry::Prompted { prompt, .. } => prompted(session, prompt),
            Entry::Opened { run, .. } => opened(session, run),
            Entry::Called { agent, call, .. } => called(session, *agent, call),
            Entry::Event { at_ms, event } => self.event(session, None, *at_ms, event),
            Entry::Shown(_)
            | Entry::Message(_)
            | Entry::Truncated { .. }
            | Entry::Amended { .. }
            | Entry::Restarted => {}
        }
    }

    fn event(&self, session: &str, agent: Option<usize>, at_ms: u64, event: &EngineEvent) {
        match event {
            EngineEvent::AgentEvent { index, event } => self.event(session, Some(*index), at_ms, event),
            EngineEvent::ToolStarted(call) => self.tool_started(session, agent, at_ms, call),
            EngineEvent::ToolResult { id, output, failed } => {
                let open = self.tools.locked().remove(id);
                let name = open.as_ref().map_or("", |open| open.name.as_str());
                let duration_ms = open.as_ref().map(|open| at_ms.saturating_sub(open.at_ms));
                tool_call(session, agent, &Finished { id, name, duration_ms, output, failed: *failed });
            }
            EngineEvent::Usage(usage) => used(session, agent, usage),
            other => {
                if !step(session, agent, other) && !changed(session, agent, other) && !ended(session, agent, other) {
                    rest(session, agent, other);
                }
            }
        }
    }

    fn tool_started(&self, session: &str, agent: Option<usize>, at_ms: u64, call: &ToolCall) {
        let mut tools = self.tools.locked();
        if tools.len() >= MOST_OPEN_TOOLS {
            tools.clear();
        }
        tools.insert(call.id.clone(), Open { name: call.name.clone(), at_ms });
        drop(tools);
        let input = call.input.as_ref().map(|input| preview(&input.to_string())).unwrap_or_default();
        tracing::event!(
            target: "audit", tracing::Level::DEBUG,
            session.id = session,
            agent.index = agent,
            gen_ai.tool.name = %call.name,
            gen_ai.tool.call.id = %call.id,
            gen_ai.tool.call.arguments = %input,
            "tool.start"
        );
    }
}

struct Finished<'a> {
    id: &'a str,
    name: &'a str,
    duration_ms: Option<u64>,
    output: &'a str,
    failed: bool,
}

fn tool_call(session: &str, agent: Option<usize>, done: &Finished<'_>) {
    let (id, name, duration_ms) = (done.id, done.name, done.duration_ms);
    if done.failed {
        let error = preview(done.output);
        tracing::event!(target: "audit", tracing::Level::WARN, session.id = session, agent.index = agent, gen_ai.tool.name = name, gen_ai.tool.call.id = id, duration_ms, outcome = "failure", error.message = %error, "tool.call");
    } else {
        tracing::event!(target: "audit", tracing::Level::INFO, session.id = session, agent.index = agent, gen_ai.tool.name = name, gen_ai.tool.call.id = id, duration_ms, outcome = "success", "tool.call");
        tracing::event!(target: "audit", tracing::Level::DEBUG, session.id = session, gen_ai.tool.call.id = id, gen_ai.tool.call.result = %preview(done.output), "tool.result");
    }
}

fn prompted(session: &str, prompt: &str) {
    tracing::event!(target: "audit", tracing::Level::INFO, session.id = session, prompt.chars = prompt.chars().count(), "agent.prompt");
    tracing::event!(target: "audit", tracing::Level::DEBUG, session.id = session, prompt.text = %preview(prompt), "agent.prompt.content");
}

fn opened(session: &str, run: &RunInfo) {
    tracing::event!(
        target: "audit", tracing::Level::INFO,
        session.id = session,
        gen_ai.provider.name = %run.provider,
        gen_ai.request.model = %run.model,
        gen_ai.request.effort = %run.effort,
        permission.mode = %run.permission,
        process.working_directory = %run.cwd,
        vcs.ref.head.name = run.branch.as_deref(),
        "agent.run.start"
    );
}

fn called(session: &str, agent: Option<usize>, call: &ModelCall) {
    tracing::event!(
        target: "audit", tracing::Level::INFO,
        session.id = session,
        agent.index = agent,
        gen_ai.request.model = %call.model,
        gen_ai.response.model = call.served.as_deref(),
        gen_ai.response.finish_reasons = %call.stop,
        gen_ai.request.max_tokens = call.max_tokens,
        gen_ai.request.effort = %call.effort,
        gen_ai.request.messages = call.messages,
        "gen_ai.call"
    );
}

fn used(session: &str, agent: Option<usize>, usage: &ModelUsage) {
    tracing::event!(
        target: "audit", tracing::Level::INFO,
        session.id = session,
        agent.index = agent,
        gen_ai.provider.name = %usage.provider,
        gen_ai.response.model = %usage.model,
        gen_ai.usage.input_tokens = usage.input,
        gen_ai.usage.output_tokens = usage.output,
        gen_ai.usage.cache_read_tokens = usage.cache_read,
        gen_ai.usage.cache_write_tokens = usage.cache_write,
        cost.usd = usage.estimated_cost_usd,
        "gen_ai.usage"
    );
}

fn step(session: &str, agent: Option<usize>, event: &EngineEvent) -> bool {
    match event {
        EngineEvent::Asked(ask) => asked(session, agent, ask),
        EngineEvent::Answered { ask, decision } => {
            tracing::event!(target: "audit", tracing::Level::INFO, session.id = session, agent.index = agent, ask.id = ?ask, ask.decision = ?decision, "agent.ask.answer");
        }
        EngineEvent::Refused { tool, reason } => {
            tracing::event!(target: "audit", tracing::Level::WARN, session.id = session, agent.index = agent, gen_ai.tool.name = %tool, error.message = %preview(reason), "tool.denied");
        }
        EngineEvent::AgentUpdate { index, status, .. } => subagent(session, *index, status),
        _ => return false,
    }
    true
}

fn changed(session: &str, agent: Option<usize>, event: &EngineEvent) -> bool {
    match event {
        EngineEvent::Edited(edit) => {
            tracing::event!(target: "audit", tracing::Level::INFO, session.id = session, agent.index = agent, file.path = %edit.path.display(), lines.added = edit.added, lines.removed = edit.removed, "file.edit");
        }
        EngineEvent::Compacted(done) => {
            tracing::event!(target: "audit", tracing::Level::INFO, session.id = session, agent.index = agent, context.tokens.before = done.before, context.tokens.after = done.after, "agent.compact");
        }
        EngineEvent::AgentSpawned { index, kind, .. } => {
            tracing::event!(target: "audit", tracing::Level::INFO, session.id = session, agent.index = index, agent.kind = %kind, "subagent.start");
        }
        _ => return false,
    }
    true
}

fn subagent(session: &str, index: usize, status: &AgentStatus) {
    match status {
        AgentStatus::Running { .. } => {}
        AgentStatus::Done { seconds } => {
            let duration_ms = u64::from(*seconds) * 1000;
            tracing::event!(target: "audit", tracing::Level::INFO, session.id = session, agent.index = index, duration_ms, outcome = "success", "subagent.end");
        }
        AgentStatus::Failed => {
            tracing::event!(target: "audit", tracing::Level::WARN, session.id = session, agent.index = index, outcome = "failure", "subagent.end");
        }
        AgentStatus::Cancelled => {
            tracing::event!(target: "audit", tracing::Level::INFO, session.id = session, agent.index = index, outcome = "cancelled", "subagent.end");
        }
    }
}

fn asked(session: &str, agent: Option<usize>, ask: &AgentAsk) {
    let (kind, tool, effects, recipients) = match &ask.kind {
        AskKind::Permission { tool, disclosed, .. } => {
            let effects: Vec<&str> = disclosed.effects.iter().map(|effect| effect.word()).collect();
            ("permission", Some(tool.as_str()), effects.join(","), disclosed.recipients.len())
        }
        AskKind::Questions(_) => ("questions", None, String::new(), 0),
        AskKind::Plan { .. } => ("plan", None, String::new(), 0),
    };
    tracing::event!(target: "audit", tracing::Level::INFO, session.id = session, agent.index = agent, ask.id = ?ask.id, ask.kind = kind, gen_ai.tool.name = tool, ask.effects = effects, ask.recipients = recipients, "agent.ask");
}

fn ended(session: &str, agent: Option<usize>, event: &EngineEvent) -> bool {
    match event {
        EngineEvent::Completed { duration_ms, cost_usd } => {
            tracing::event!(target: "audit", tracing::Level::INFO, session.id = session, agent.index = agent, duration_ms, cost.usd = cost_usd, outcome = "success", "agent.run.end");
        }
        EngineEvent::Failed(reason) => {
            tracing::event!(target: "audit", tracing::Level::ERROR, session.id = session, agent.index = agent, outcome = "failure", error.message = %preview(reason), "agent.run.end");
        }
        EngineEvent::Cancelled => {
            tracing::event!(target: "audit", tracing::Level::INFO, session.id = session, agent.index = agent, outcome = "cancelled", "agent.run.end");
        }
        _ => return false,
    }
    true
}

fn rest(session: &str, agent: Option<usize>, event: &EngineEvent) {
    match event {
        EngineEvent::FellBack(reason) => {
            tracing::event!(target: "audit", tracing::Level::WARN, session.id = session, agent.index = agent, error.message = %preview(reason), "agent.fallback");
        }
        EngineEvent::Rerouted(rerouted) => {
            let skipped = serde_json::to_string(&rerouted.skipped).unwrap_or_default();
            tracing::event!(target: "audit", tracing::Level::WARN, session.id = session, agent.index = agent, model.from = %rerouted.from, model.to = rerouted.to.as_deref(), error.kind = %rerouted.failure, model.skipped = %skipped, "agent.fallback.decision");
        }
        EngineEvent::AccountSwitched { from, to } => {
            tracing::event!(target: "audit", tracing::Level::WARN, session.id = session, agent.index = agent, account.from = %from, account.to = %to, "agent.account.switch");
        }
        other => {
            tracing::event!(target: "audit", tracing::Level::TRACE, session.id = session, agent.index = agent, event.kind = %kind(other), "agent.event");
        }
    }
}

fn kind(event: &EngineEvent) -> String {
    let described = format!("{event:?}");
    let end = described.find(['(', ' ', '{']).unwrap_or(described.len());
    described[..end].to_owned()
}

fn preview(text: &str) -> String {
    let flat: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
    match flat.char_indices().nth(PREVIEW_CHARS) {
        Some((cut, _)) => format!("{}…", &flat[..cut]),
        None => flat,
    }
}

#[cfg(test)]
mod tests {
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

    #[test]
    fn a_tool_is_closed_by_its_result_even_inside_a_subagent() {
        let audit = Audit::default();
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
}
