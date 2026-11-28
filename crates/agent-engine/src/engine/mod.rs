mod audit;
pub(crate) mod date;
mod job;
mod live;
mod phases;
mod record;
mod rewind;
pub(crate) mod roles;
mod sent;
mod session;
mod small_model;
pub use small_model::pick as pick_small_model;
mod snapshot;
mod stop;
mod task;
#[cfg(test)]
mod tests;

use std::sync::Arc;

use self::job::Job;
use crate::boundary::{AgentEngine, EngineError, EventSink, RunGate, RunHandle};
use crate::control::Control;
use crate::lsp::{LanguageServers, LspPool};
use crate::model::http::{Http, NetHttp};
use crate::model::{ModelApi, ModelError};
use crate::sessions::Sessions;
use crate::tools::Workspace;
use domain::{Decision, EngineEvent, ModelOption, PermissionMode, PromptRequest, RunId};

const MAX_OUTPUT: u32 = 64_000;

pub trait ApiSource: Send + Sync {
    fn api(&self, request: &PromptRequest) -> Result<Arc<dyn ModelApi>, ModelError>;

    fn catalog(&self, _request: &PromptRequest) -> Vec<ModelOption> {
        Vec::new()
    }

    fn searcher(&self, _request: &PromptRequest) -> Option<Arc<dyn crate::model::Searcher>> {
        None
    }
}

pub struct Engine {
    source: Arc<dyn ApiSource>,
    sessions: Arc<dyn Sessions>,
    mcp: Arc<dyn Http>,
    lsp: Arc<dyn LanguageServers>,
    pointers: Option<crate::tools::CodePointers>,
    history: Option<crate::history::HostHistory>,
    launch: crate::tools::Launch,
    live: Arc<live::LiveSessions>,
    runtime: Result<tokio::runtime::Handle, String>,
    redactor: Arc<domain::redact::Redactor>,
}

impl std::fmt::Debug for Engine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Engine").finish_non_exhaustive()
    }
}

impl Engine {
    pub fn new(source: Arc<dyn ApiSource>, sessions: Arc<dyn Sessions>) -> Self {
        crate::tools::persisted::forget_old_results_once();
        Self {
            source,
            sessions,
            mcp: Arc::new(NetHttp),
            lsp: Arc::new(LspPool::default()),
            pointers: None,
            history: None,
            launch: crate::tools::Launch::default(),
            live: Arc::default(),
            runtime: task::runtime().map_err(|error| error.to_string()),
            redactor: crate::tools::redactor(),
        }
    }

    pub fn with_redactor(mut self, redactor: Arc<domain::redact::Redactor>) -> Self {
        self.redactor = redactor;
        self
    }

    pub fn with_language_servers(mut self, lsp: Arc<dyn LanguageServers>) -> Self {
        self.lsp = lsp;
        self
    }

    pub fn with_pointers(mut self, pointers: Arc<dyn crate::tools::Pointers>) -> Self {
        self.pointers = Some(crate::tools::CodePointers(pointers));
        self
    }

    pub fn with_history(mut self, history: Arc<dyn crate::history::History>) -> Self {
        self.history = Some(crate::history::HostHistory(history));
        self
    }
}

impl AgentEngine for Engine {
    fn models(&self) -> Vec<ModelOption> {
        Vec::new()
    }

    fn start(
        &self,
        run: RunId,
        request: &PromptRequest,
        sink: Arc<dyn EventSink>,
    ) -> Result<Box<dyn RunHandle>, EngineError> {
        let mut workspace = Workspace::for_request(request);
        workspace.services.launch = self.launch.clone();
        workspace.services.pointers.clone_from(&self.pointers);
        workspace.services.history.clone_from(&self.history);
        let control = Arc::new(
            Control::new(request.policy.permission, workspace.stop.clone()).reviewed_by(request.policy.reviewer),
        );
        let detach = Arc::clone(&workspace.detach);
        let gate = RunGate::new(run, sink);
        let job = Job {
            source: Arc::clone(&self.source),
            sessions: Arc::clone(&self.sessions),
            mcp: Arc::clone(&self.mcp),
            lsp: Arc::clone(&self.lsp),
            request: request.clone(),
            control: Arc::clone(&control),
            gate: gate.clone(),
            recorder: None,
            held: Arc::default(),
            live: live::Live::of(&self.live),
            redactor: Arc::clone(&self.redactor),
        };
        let runtime = self
            .runtime
            .as_ref()
            .map_err(|why| EngineError::Spawn { engine: "the engine", source: std::io::Error::other(why.clone()) })?;
        task::spawn(runtime, job, workspace, Arc::clone(&control), gate.clone());
        Ok(Box::new(Handle { control, gate, detach }))
    }
}

struct Handle {
    control: Arc<Control>,
    gate: RunGate,
    detach: Arc<std::sync::atomic::AtomicBool>,
}

impl RunHandle for Handle {
    fn cancel(&mut self) {
        self.control.cancel();
    }

    fn answer(&mut self, ask: &domain::AskId, decision: &Decision) {
        self.control.answer(ask, decision.clone());
    }

    fn set_mode(&mut self, mode: PermissionMode) {
        self.control.set_mode(mode);
    }

    fn steer(&mut self, text: String) {
        self.control.steer(text);
    }

    fn unsteer(&mut self) {
        self.gate.emit(EngineEvent::Unsteered(self.control.take_steered()));
    }

    fn background(&mut self) {
        self.detach.store(true, std::sync::atomic::Ordering::SeqCst);
    }
}
