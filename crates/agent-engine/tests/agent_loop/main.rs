#![allow(clippy::expect_used, reason = "a failed setup step stops the test")]

mod background_reports;
mod code_review;
mod compaction;
mod custom_agents;
mod data_policy;
mod escalation;
mod follow_through;
mod input_validation;
mod intercepted_model;
mod intercepted_tools;
mod interception;
mod lsp_install;
mod model_hooks;
mod native_search;
mod observers;
mod permissions;
mod plan_mode;
mod redaction;
mod repeats;
mod repl;
mod safeguards;
mod script_skills;
mod scripts;
mod send_message;
mod steering;
mod sub_agent_asks;
mod sub_agent_endings;
mod sub_agent_fallbacks;
mod sub_agents;
mod teams;
mod turns;
mod two;
mod watch;
mod watch_hooks;
mod write_boundary;

use std::sync::{Arc, Mutex};

use agent_engine::agent::{Agent, Ending};
use agent_engine::control::Control;
use agent_engine::model::TurnRequest;
use agent_engine::model::scripted::ScriptedModel;
use agent_engine::tools::{self, Workspace};
use agent_engine::transcript::{Block, Message};
use domain::{Effort, EngineEvent, PermissionMode};

fn request() -> TurnRequest {
    TurnRequest {
        model: "m".into(),
        system: "s".into(),
        messages: Vec::new(),
        tools: Agent::specs(&tools::standard()),
        effort: Effort::Medium,
        max_tokens: 1000,
        intent: Default::default(),
    }
}

fn tool_use(id: &str, name: &str, input: serde_json::Value) -> Block {
    Block::ToolUse { id: id.into(), name: name.into(), input }
}

struct Rig {
    dir: tempfile::TempDir,
    workspace: Workspace,
    control: Arc<Control>,
    events: Arc<Mutex<Vec<EngineEvent>>>,
}

impl Rig {
    fn new(reviewer: domain::Reviewer) -> Self {
        Self::with(PermissionMode::Auto, reviewer)
    }

    fn planning() -> Self {
        Self::with(PermissionMode::Plan, domain::Reviewer::Human)
    }

    fn with(mode: PermissionMode, reviewer: domain::Reviewer) -> Self {
        let dir = tempfile::tempdir().expect("dir");
        std::fs::write(dir.path().join("notes.txt"), "alpha\nbeta\n").expect("write");
        let workspace = Workspace::new(dir.path());
        let control = Arc::new(Control::new(mode, Arc::default()).reviewed_by(reviewer));
        Self { dir, workspace, control, events: Arc::default() }
    }

    fn run(&self, model: &ScriptedModel, history: &mut Vec<Message>) -> Result<(), Ending> {
        self.run_with(model, history, Vec::new())
    }

    fn run_with(
        &self,
        model: &ScriptedModel,
        history: &mut Vec<Message>,
        extra: Vec<Box<dyn tools::Tool>>,
    ) -> Result<(), Ending> {
        self.run_hiding(model, history, extra, &[])
    }

    fn run_hiding(
        &self,
        model: &ScriptedModel,
        history: &mut Vec<Message>,
        extra: Vec<Box<dyn tools::Tool>>,
        hidden: &[&str],
    ) -> Result<(), Ending> {
        let mut tools = tools::standard();
        tools.extend(extra);
        let events = Arc::clone(&self.events);
        let emit = move |event: EngineEvent| events.lock().expect("events").push(event);
        let offered = Agent::specs(&tools).into_iter().filter(|spec| !hidden.contains(&spec.name.as_str())).collect();
        let agent = Agent {
            api: model,
            tools: &tools,
            workspace: &self.workspace,
            control: &self.control,
            emit: &emit,
            request: TurnRequest { tools: offered, ..request() },
            agents: &[],
            depth: 0,
            turn_limit: agent_engine::agent::MAX_TURNS,
        };
        agent.run(history).map(|_| ())
    }

    fn events(&self) -> Vec<EngineEvent> {
        self.events.lock().expect("events").clone()
    }
}

pub(crate) struct Answerer {
    done: Arc<std::sync::atomic::AtomicBool>,
    thread: std::thread::JoinHandle<usize>,
}

impl Answerer {
    pub(crate) fn join(self) -> std::thread::Result<()> {
        self.done.store(true, std::sync::atomic::Ordering::SeqCst);
        self.thread.join().map(|_| ())
    }

    pub(crate) fn answered(self) -> usize {
        self.done.store(true, std::sync::atomic::Ordering::SeqCst);
        self.thread.join().expect("answerer")
    }
}

pub(crate) fn answering(rig: &Rig, decision: domain::Decision) -> Answerer {
    let control = Arc::clone(&rig.control);
    let events = Arc::clone(&rig.events);
    let done = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let finished = Arc::clone(&done);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
    let thread = std::thread::spawn(move || {
        let mut seen = std::collections::BTreeSet::new();
        while !finished.load(std::sync::atomic::Ordering::SeqCst) && std::time::Instant::now() < deadline {
            let asks: Vec<_> = events
                .lock()
                .expect("events")
                .iter()
                .filter_map(|event| match event {
                    EngineEvent::Asked(ask) => Some(ask.id.clone()),
                    _ => None,
                })
                .collect();
            for id in asks {
                control.answer(&id, decision.clone());
                seen.insert(id.0.clone());
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        seen.len()
    });
    Answerer { done, thread }
}
