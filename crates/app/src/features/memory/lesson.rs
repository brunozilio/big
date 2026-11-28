use std::path::PathBuf;
use std::sync::{Arc, Mutex, mpsc};
use std::time::Duration;

use domain::learning::{Keeping, Lesson, Proposal, Signals};
use domain::{
    Author, ChoreAnswer, EngineEvent, MemoryEdit, MemoryKind, ModelOption, PromptRequest, Provenance, RunId,
    SessionCommand, SessionId,
};
use platform::sync::Locked;

use crate::services::LocalServices;

const WAIT: Duration = Duration::from_secs(120);
const ORIGIN: &str = "lesson";

pub(crate) struct Learning {
    services: LocalServices,
    cwd: PathBuf,
    model: ModelOption,
    signals: Mutex<Signals>,
    session: Mutex<Option<SessionId>>,
}

impl Learning {
    pub fn of(services: &LocalServices, request: &PromptRequest) -> Option<Self> {
        let wanted = request.session.command.is_none() && request.host.memory.is_some();
        wanted.then(|| Self {
            services: services.clone(),
            cwd: request.session.cwd.clone(),
            model: request.session.model.clone(),
            signals: Mutex::default(),
            session: Mutex::new(request.session.resume.clone()),
        })
    }

    pub fn observe(&self, event: &EngineEvent) {
        self.signals.locked().observe(event);
        if let EngineEvent::SessionStarted(session) = event {
            *self.session.locked() = Some(session.clone());
        }
        if !matches!(event, EngineEvent::Completed { .. }) {
            return;
        }
        let Some(brief) = self.signals.locked().lesson_due() else { return };
        let Some(session) = self.session.locked().clone() else { return };
        let (services, cwd, model) = (self.services.clone(), self.cwd.clone(), self.model.clone());
        let started = std::thread::Builder::new().name("lesson".into()).spawn(move || {
            if let Some(proposal) = proposed(&services, cwd.clone(), model, brief, session) {
                keep(&services, cwd, &proposal);
            }
        });
        if let Err(error) = started {
            tracing::warn!(%error, "a lesson could not be started");
        }
    }
}

fn proposed(
    services: &LocalServices,
    cwd: PathBuf,
    model: ModelOption,
    brief: String,
    session: SessionId,
) -> Option<Proposal> {
    let mut request = PromptRequest::plain(cwd, brief, model);
    request.session.resume = Some(session);
    request.session.command = Some(SessionCommand::Learn);
    let run = RunId::new();
    let prepared = crate::prepare::prepare(services, 0, run, &request);
    let (sender, receiver) = mpsc::channel();
    let sink: Arc<dyn agent_engine::EventSink> = Arc::new(move |_: RunId, event: EngineEvent| {
        let _ = sender.send(event);
    });
    let _handle = services.engine.start(run, &prepared.request, sink).ok()?;
    let windows = llm_providers::batch::Windows::configured();
    while let Ok(event) = receiver.recv_timeout(WAIT + windows.gather + windows.wait) {
        match event {
            EngineEvent::Chore(ChoreAnswer::Learned(proposal)) => return Some(proposal),
            event if event.is_terminal() => return None,
            _ => {}
        }
    }
    None
}

pub(crate) fn edit_of(proposal: &Proposal) -> MemoryEdit {
    let (kind, text) = match proposal.kind {
        Lesson::Memory => (MemoryKind::Semantic, proposal.text.clone()),
        Lesson::Instruction => (MemoryKind::Procedural, proposal.text.clone()),
        Lesson::Skill | Lesson::Script => (MemoryKind::Procedural, format!("{}: {}", proposal.name, proposal.text)),
    };
    MemoryEdit::Remember { kind, text, tags: vec![ORIGIN.to_owned()], touches: Vec::new(), scope: Default::default() }
}

fn keep(services: &LocalServices, cwd: PathBuf, proposal: &Proposal) {
    if let Keeping::Propose { name, source } = domain::learning::keeping(proposal) {
        if let Err(error) = agent_engine::tools::bank::propose(&name, &source) {
            tracing::warn!(%error, name, "a proposed script could not be written for approval");
        }
        return;
    }
    let by = Provenance { author: Author::Agent, now: super::now(), origin: Some(ORIGIN.to_owned()), lineage: None };
    if let Err(error) = super::of(services).edit(&cwd, &[edit_of(proposal)], by) {
        tracing::warn!(%error, "a lesson could not be kept in memory");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_kind_of_lesson_is_kept_as_a_memory_of_the_project() {
        let lesson = |kind, name: &str| Proposal { kind, name: name.into(), text: "use pnpm".into(), why: "x".into() };
        let text_of = |edit: MemoryEdit| match edit {
            MemoryEdit::Remember { kind, text, tags, .. } => (kind, text, tags),
            _ => (MemoryKind::Episodic, String::new(), Vec::new()),
        };
        assert_eq!(text_of(edit_of(&lesson(Lesson::Memory, ""))).0, MemoryKind::Semantic);
        assert_eq!(text_of(edit_of(&lesson(Lesson::Instruction, ""))).0, MemoryKind::Procedural);
        let (kind, text, tags) = text_of(edit_of(&lesson(Lesson::Skill, "install")));
        assert_eq!((kind, text.as_str()), (MemoryKind::Procedural, "install: use pnpm"));
        assert_eq!(tags, vec![ORIGIN.to_owned()]);
    }
}
