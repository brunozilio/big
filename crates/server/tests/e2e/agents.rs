use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use agent_engine::{AgentEngine, EngineError, EventSink, RunHandle};
use domain::{
    AgentAsk, AskId, AskKind, Decision, EngineEvent, ModelOption, PermissionMode, PromptRequest, RunId, SessionId,
};
use serde_json::json;

pub const PLAN: &str = "1. Criar work.txt\n2. Testar";

#[derive(Debug, Default)]
pub struct Script {
    pub runs: Mutex<Vec<(String, String, String)>>,
    pub hold_develop: AtomicBool,
    pub ask_develop: AtomicBool,
    pub started: AtomicUsize,
    pub answers: Mutex<Vec<(String, Decision)>>,
}

#[derive(Debug, Clone, Default)]
pub struct Agents {
    pub script: Arc<Script>,
}

impl Agents {
    pub fn runs_of(&self, role: &str) -> Vec<(String, String)> {
        let runs = self.script.runs.lock().unwrap_or_else(PoisonError::into_inner);
        runs.iter()
            .filter(|(found, _, _)| found == role)
            .map(|(_, session, prompt)| (session.clone(), prompt.clone()))
            .collect()
    }
}

struct Handle {
    cancelled: Arc<AtomicBool>,
    answer: Sender<Decision>,
    script: Arc<Script>,
}

impl RunHandle for Handle {
    fn cancel(&mut self) {
        self.cancelled.store(true, Ordering::SeqCst);
    }

    fn answer(&mut self, ask: &AskId, decision: &Decision) {
        self.script.answers.lock().unwrap_or_else(PoisonError::into_inner).push((ask.0.clone(), decision.clone()));
        let _ = self.answer.send(decision.clone());
    }
}

fn commit(cwd: &Path, number: usize) {
    let _ = std::fs::write(cwd.join(format!("work-{number}.txt")), format!("feito {number}\n"));
    for args in [vec!["add", "-A"], vec!["commit", "-qm", "agente"]] {
        let _ = std::process::Command::new("git")
            .arg("-C")
            .arg(cwd)
            .args(["-c", "user.email=a@b", "-c", "user.name=agente"])
            .args(args)
            .output();
    }
}

struct Turn {
    sink: Arc<dyn EventSink>,
    run: RunId,
    cancelled: Arc<AtomicBool>,
    answers: Receiver<Decision>,
}

impl Turn {
    fn emit(&self, event: EngineEvent) {
        self.sink.emit(self.run, event);
    }

    fn ask(&self, id: String, kind: AskKind) -> Option<Decision> {
        self.emit(EngineEvent::Asked(AgentAsk { id: AskId(id), kind }));
        loop {
            if self.cancelled.load(Ordering::SeqCst) {
                return None;
            }
            if let Ok(decision) = self.answers.recv_timeout(Duration::from_millis(10)) {
                return Some(decision);
            }
        }
    }

    fn finish(&self, output: serde_json::Value) {
        if self.cancelled.load(Ordering::SeqCst) {
            return;
        }
        self.emit(EngineEvent::TextDelta("pronto".into()));
        self.emit(EngineEvent::StructuredOutput(output));
        self.emit(EngineEvent::Completed { duration_ms: 0, cost_usd: None });
    }
}

fn plan(turn: &Turn, number: usize) {
    let Some(decision) = turn.ask(format!("plan-{number}"), AskKind::Plan { plan: PLAN.into() }) else { return };
    let approved = matches!(decision, Decision::Approve(_));
    turn.finish(json!({ "plan": PLAN, "title": "Entrega do item", "approved": approved }));
}

fn develop(turn: &Turn, script: &Script, places: &[PathBuf], number: usize) {
    if script.ask_develop.swap(false, Ordering::SeqCst) {
        let kind = AskKind::Permission { tool: "Bash".into(), subject: "rm -rf build".into(), rememberable: false };
        if turn.ask(format!("perm-{number}"), kind).is_none() {
            return;
        }
    }
    while script.hold_develop.load(Ordering::SeqCst) && !turn.cancelled.load(Ordering::SeqCst) {
        std::thread::sleep(Duration::from_millis(10));
    }
    if turn.cancelled.load(Ordering::SeqCst) {
        return;
    }
    for place in places {
        commit(place, number);
    }
    turn.finish(json!({ "summary": format!("mudança {number}") }));
}

impl AgentEngine for Agents {
    fn models(&self) -> Vec<ModelOption> {
        vec![ModelOption::new("m1", "Modelo")]
    }

    fn start(
        &self,
        run: RunId,
        request: &PromptRequest,
        sink: Arc<dyn EventSink>,
    ) -> Result<Box<dyn RunHandle>, EngineError> {
        let number = self.script.started.fetch_add(1, Ordering::SeqCst) + 1;
        let role = if request.policy.permission == PermissionMode::Plan { "planner" } else { "developer" };
        let session = request.session.resume.clone().unwrap_or_else(|| SessionId(format!("{role}-{number}")));
        self.script.runs.lock().unwrap_or_else(PoisonError::into_inner).push((
            role.to_owned(),
            session.0.clone(),
            request.prompt.text.clone(),
        ));
        let cancelled = Arc::new(AtomicBool::new(false));
        let (answer, answers) = channel();
        let turn = Turn { sink, run, cancelled: Arc::clone(&cancelled), answers };
        let script = Arc::clone(&self.script);
        let places: Vec<PathBuf> =
            std::iter::once(request.session.cwd.clone()).chain(request.session.add_dirs.iter().cloned()).collect();
        let command = request.session.command.is_some();
        std::thread::spawn(move || {
            turn.emit(EngineEvent::SessionStarted(session));
            if command {
                turn.emit(EngineEvent::Completed { duration_ms: 0, cost_usd: None });
                return;
            }
            match role {
                "planner" => plan(&turn, number),
                _ => develop(&turn, &script, &places, number),
            }
        });
        Ok(Box::new(Handle { cancelled, answer, script: Arc::clone(&self.script) }))
    }
}
