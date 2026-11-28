use std::collections::HashMap;
use std::sync::Mutex;

use domain::billing::ModelUsage;
use domain::trace::capture::Keys;
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

pub(super) fn around(request: &domain::PromptRequest) -> tracing::Span {
    let agent = request.session.agent.as_ref().map(|spec| spec.name.as_str());
    let user = request.host.owner.as_deref();
    let definition = request.session.agent.as_ref().and_then(|spec| spec.definition.as_deref());
    let version = definition.map(|text| domain::factory::generation([("agent", text)]));
    let creator = domain::audit::CreatorType::Agent.word();
    tracing::info_span!(target: "audit", "agent.run", agent, agent.version = version.as_deref(), user, creator.type = creator)
}

pub(super) struct Audit {
    tools: Mutex<HashMap<String, Open>>,
    keys: Keys,
}

impl Default for Audit {
    fn default() -> Self {
        Self::capturing(Keys::configured())
    }
}

impl Audit {
    fn capturing(keys: Keys) -> Self {
        Self { tools: Mutex::default(), keys }
    }

    pub(super) fn entry(&self, session: &SessionId, entry: &Entry) {
        let session = session.0.as_str();
        match entry {
            Entry::Prompted { prompt, .. } => prompted(session, prompt, self.keys),
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
                let finished = Finished { id, name, duration_ms, output, failed: *failed };
                tool_call(session, agent, &finished, self.keys);
            }
            EngineEvent::Usage(usage) if self.keys.usage => used(session, agent, usage),
            EngineEvent::Usage(_) => {}
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
        let input = call.input.as_ref().filter(|_| self.keys.content).map(|input| preview(&input.to_string()));
        tracing::event!(
            target: "audit", tracing::Level::DEBUG,
            session.id = session,
            agent.index = agent,
            gen_ai.tool.name = %call.name,
            gen_ai.tool.call.id = %call.id,
            gen_ai.tool.call.arguments = input,
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

fn tool_call(session: &str, agent: Option<usize>, done: &Finished<'_>, keys: Keys) {
    let (id, name, duration_ms) = (done.id, done.name, done.duration_ms);
    if done.failed {
        let error = preview(done.output);
        tracing::event!(target: "audit", tracing::Level::WARN, session.id = session, agent.index = agent, gen_ai.tool.name = name, gen_ai.tool.call.id = id, duration_ms, outcome = "failure", error.message = %error, "tool.call");
    } else {
        tracing::event!(target: "audit", tracing::Level::INFO, session.id = session, agent.index = agent, gen_ai.tool.name = name, gen_ai.tool.call.id = id, duration_ms, outcome = "success", "tool.call");
        if keys.content {
            tracing::event!(target: "audit", tracing::Level::DEBUG, session.id = session, gen_ai.tool.call.id = id, gen_ai.tool.call.result = %preview(done.output), "tool.result");
        }
    }
}

fn prompted(session: &str, prompt: &str, keys: Keys) {
    tracing::event!(target: "audit", tracing::Level::INFO, session.id = session, prompt.chars = prompt.chars().count(), "agent.prompt");
    if keys.prompts {
        tracing::event!(target: "audit", tracing::Level::DEBUG, session.id = session, prompt.text = %preview(prompt), "agent.prompt.content");
    }
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
            tracing::event!(target: "audit", tracing::Level::WARN, session.id = session, agent.index = agent, gen_ai.tool.name = %tool, outcome = "denied", error.message = %preview(reason), "tool.denied");
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
#[path = "audit_tests.rs"]
mod tests;
