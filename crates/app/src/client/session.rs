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
    pub output_style: Option<String>,
    pub language: Option<domain::Language>,
    pub owner: Option<String>,
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
        self.send_from(prompt, None)
    }

    pub fn send_from(&self, prompt: &str, from: Option<&str>) -> Result<Run, Error> {
        let mut request = PromptRequest::plain(self.cwd.clone(), prompt, self.model.clone());
        request.prompt.shown = from.map(|from| domain::request::Shown {
            text: prompt.to_owned(),
            attached: Vec::new(),
            from: Some(from.to_owned()),
        });
        request.session.effort = self.effort;
        request.host.memory = Some(MemoryBrief::new(None));
        request.host.owner.clone_from(&self.headless.owner);
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

    pub(crate) fn standalone_chore(&self, command: domain::SessionCommand, prompt: &str) -> Result<Run, Error> {
        let mut request = PromptRequest::plain(self.cwd.clone(), prompt, self.model.clone());
        request.session.command = Some(command);
        self.start(request)
    }

    fn start(&self, mut request: PromptRequest) -> Result<Run, Error> {
        request.policy.max_turns = self.headless.max_turns;
        request.session.append_system.clone_from(&self.headless.append_system);
        request.session.fallback_model.clone_from(&self.headless.fallback_model);
        request.session.teammate.clone_from(&self.headless.teammate);
        request.session.output_style.clone_from(&self.headless.output_style);
        request.session.language = self.headless.language;
        let watch = RunWatch::of(request.session.command);
        let run = Run::start(&self.dispatcher, |run| ClientCommand::SubmitPrompt { run, request })?;
        Ok(run.of_session(Arc::clone(&self.carried), watch))
    }
}
