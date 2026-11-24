use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use platform::sync::Locked;

use crate::api::ClientCommand;
use domain::memory::MemoryBrief;
use domain::session_run::{Carried, RunWatch};
use domain::{Effort, ModelOption, PermissionMode, PromptRequest, SessionId};

use crate::Error;
use crate::client::dispatch::Dispatcher;
use crate::client::run::Run;

#[derive(Debug)]
pub struct Session {
    dispatcher: Arc<Dispatcher>,
    cwd: PathBuf,
    model: ModelOption,
    effort: Effort,
    carried: Arc<Mutex<Carried>>,
    headless: Headless,
    agent: Option<domain::MainAgentSpec>,
}

#[derive(Debug, Clone, Default)]
pub struct Headless {
    pub max_turns: Option<u32>,
    pub append_system: Option<String>,
    pub fallback_model: Option<String>,
    pub teammate: Option<domain::team::TeammateOf>,
}

impl Session {
    pub(crate) fn new(dispatcher: Arc<Dispatcher>, cwd: PathBuf, model: ModelOption) -> Self {
        Self {
            dispatcher,
            cwd,
            model,
            effort: Effort::Medium,
            carried: Arc::default(),
            headless: Headless::default(),
            agent: None,
        }
    }

    pub fn set_headless(&mut self, headless: Headless) {
        self.headless = headless;
    }

    pub fn effort(mut self, effort: Effort) -> Self {
        self.effort = effort;
        self
    }

    pub fn resume(self, session: SessionId) -> Self {
        self.carried.locked().id = Some(session);
        self
    }

    pub fn agent(mut self, agent: Option<domain::MainAgentSpec>) -> Self {
        self.agent = agent;
        self
    }

    pub fn set_permission(&mut self, mode: PermissionMode) {
        self.carried.locked().mode = mode;
    }

    pub fn id(&self) -> Option<SessionId> {
        self.carried.locked().id.clone()
    }

    pub fn send(&self, prompt: &str) -> Result<Run, Error> {
        let mut request = PromptRequest::plain(self.cwd.clone(), prompt, self.model.clone());
        request.session.effort = self.effort;
        request.host.memory = Some(MemoryBrief::new(None));
        request.session.agent.clone_from(&self.agent);
        {
            let carried = self.carried.locked();
            request.policy.permission = carried.mode;
            request.session.resume = carried.id.clone();
        }
        self.start(request)
    }

    pub(crate) fn chore(&self, command: domain::SessionCommand, prompt: &str) -> Result<Run, Error> {
        let mut request = PromptRequest::plain(self.cwd.clone(), prompt, self.model.clone());
        request.session.resume = Some(self.id().ok_or(Error::NothingToAsk)?);
        request.session.command = Some(command);
        self.start(request)
    }

    fn start(&self, mut request: PromptRequest) -> Result<Run, Error> {
        request.policy.max_turns = self.headless.max_turns;
        request.session.append_system.clone_from(&self.headless.append_system);
        request.session.fallback_model.clone_from(&self.headless.fallback_model);
        request.session.teammate.clone_from(&self.headless.teammate);
        let watch = RunWatch::of(request.session.command);
        let run = Run::start(&self.dispatcher, |run| ClientCommand::SubmitPrompt { run, request })?;
        Ok(run.of_session(Arc::clone(&self.carried), watch))
    }
}
