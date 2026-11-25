use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};

use platform::sync::Locked;

use crate::agent::Agent;
use crate::agent::run::ThisRun;
use crate::control::Control;
use crate::tools::Outcome;
use crate::tools::builtin_agents;
use crate::transcript::Message;

const BACKGROUND_AT_MOST: usize = 16;

pub(in crate::agent) type Spawn<'s, 'env> = &'s (dyn Fn(Box<dyn FnOnce() + Send + 'env>) + Sync);

pub(in crate::agent) type BackgroundJob<'env> =
    Box<dyn FnOnce(&Arc<Control>, &crate::tools::Workspace) -> Outcome + Send + 'env>;

#[derive(Default)]
pub(in crate::agent) struct Background(Mutex<BTreeMap<String, Arc<BackgroundAgent>>>);

pub(in crate::agent) struct CancelOnDrop<'a>(pub(in crate::agent) &'a Background);

impl Drop for CancelOnDrop<'_> {
    fn drop(&mut self) {
        self.0.cancel_all();
    }
}

pub(in crate::agent) struct BackgroundAgent {
    result: Mutex<Option<Outcome>>,
    ready: Condvar,
    collected: AtomicBool,
    control: Arc<Control>,
}

impl BackgroundAgent {
    fn started(control: Arc<Control>) -> Self {
        Self { result: Mutex::new(None), ready: Condvar::new(), collected: AtomicBool::new(false), control }
    }

    pub(in crate::agent) fn output(&self, block: bool) -> Outcome {
        self.waited(block).unwrap_or_else(|| Outcome::ok("Task is still running."))
    }

    pub(in crate::agent) fn waited(&self, block: bool) -> Option<Outcome> {
        let mut result = self.result.locked();
        while block && result.is_none() {
            result = platform::sync::wait(&self.ready, result);
        }
        if result.is_some() {
            self.collected.store(true, Ordering::Relaxed);
        }
        result.clone()
    }

    fn running(&self) -> bool {
        self.result.locked().is_none()
    }

    pub(in crate::agent) fn is_collected(&self) -> bool {
        self.collected.load(Ordering::Relaxed)
    }

    fn finished(&self, result: Outcome) {
        *self.result.locked() = Some(result);
        self.ready.notify_all();
    }
}

impl Background {
    pub(in crate::agent) fn cancel_all(&self) {
        for task in self.0.locked().values() {
            task.control.cancel();
        }
    }

    pub(in crate::agent) fn started(&self) -> Vec<(String, Arc<BackgroundAgent>)> {
        self.0.locked().iter().map(|(id, task)| (id.clone(), task.clone())).collect()
    }

    fn has_room(&self) -> bool {
        self.0.locked().len() < BACKGROUND_AT_MOST
    }

    fn start(&self, control: Arc<Control>) -> Option<(String, Arc<BackgroundAgent>)> {
        let mut tasks = self.0.locked();
        if tasks.len() >= BACKGROUND_AT_MOST {
            return None;
        }
        let id = format!("task-{}", tasks.len() + 1);
        let task = Arc::new(BackgroundAgent::started(control));
        tasks.insert(id.clone(), task.clone());
        Some((id, task))
    }

    pub(in crate::agent) fn find(&self, id: &str) -> Option<Arc<BackgroundAgent>> {
        self.0.locked().get(id).cloned()
    }

    pub(in crate::agent) fn stop(&self, id: &str) -> bool {
        let Some(task) = self.find(id) else { return false };
        task.control.cancel();
        task.waited(true);
        true
    }

    pub(in crate::agent) fn listed(&self) -> Vec<Value> {
        self.started()
            .into_iter()
            .map(|(id, task)| json!({"task_id": id, "status": if task.running() { "running" } else { "completed" }}))
            .collect()
    }
}

fn limit_reached(what: &str) -> Outcome {
    Outcome::error(format!("Background {what} limit reached (16 per run; no nested agents)."))
}

impl Agent<'_> {
    pub(in crate::agent) fn can_background(&self, run: &ThisRun) -> bool {
        self.depth == 0 && run.background.has_room()
    }

    pub(in crate::agent) fn start_background<'env>(
        &'env self,
        input: &Value,
        run: &ThisRun,
        parent: &[Message],
        spawn: Spawn<'_, 'env>,
    ) -> Result<String, Outcome> {
        let conversations = run.conversations.clone();
        let input = input.clone();
        let parent = if builtin_agents::is_fork(&input) { parent.to_vec() } else { Vec::new() };
        let job = move |control: &Arc<Control>, workspace: &crate::tools::Workspace| {
            self.sub_agent_with(&input, control, workspace, &parent, &conversations, None)
        };
        self.in_background(run, "agent", Box::new(job), spawn)
    }

    pub(in crate::agent) fn in_background<'env>(
        &'env self,
        run: &ThisRun,
        what: &str,
        job: BackgroundJob<'env>,
        spawn: Spawn<'_, 'env>,
    ) -> Result<String, Outcome> {
        if !self.can_background(run) {
            return Err(limit_reached(what));
        }
        let control = self.control.child();
        let Some((id, task)) = run.background.start(control.clone()) else { return Err(limit_reached(what)) };
        let workspace = self.workspace.fork(control.shared_stop());
        let panicked = format!("Background {what} panicked.");
        spawn(Box::new(move || {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| job(&control, &workspace)))
                .unwrap_or_else(|_| Outcome::error(panicked));
            task.finished(result);
        }));
        Ok(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn control() -> Arc<Control> {
        Arc::new(Control::new(domain::PermissionMode::Auto, Arc::default()).reviewed_by(domain::Reviewer::Human))
    }

    #[test]
    fn a_started_task_is_listed_running_then_collected_once_it_reports() {
        let background = Background::default();
        let (id, task) = background.start(control()).expect("room");
        assert_eq!(id, "task-1");
        assert_eq!(background.listed(), [json!({"task_id": "task-1", "status": "running"})]);
        let found = background.find(&id).expect("started");
        assert_eq!(found.waited(false), None, "still running");
        task.finished(Outcome::ok("report"));
        assert!(background.started().iter().all(|(_, task)| !task.is_collected()), "asking early collects nothing");
        assert_eq!(found.waited(true), Some(Outcome::ok("report")));
        assert!(background.started().iter().all(|(_, task)| task.is_collected()));
        assert_eq!(background.listed(), [json!({"task_id": "task-1", "status": "completed"})]);
        assert!(background.find("task-9").is_none() && !background.stop("task-9"));
    }

    #[test]
    fn no_more_than_sixteen_tasks_start_in_one_run() {
        let background = Background::default();
        for _ in 0..BACKGROUND_AT_MOST {
            assert!(background.start(control()).is_some());
        }
        assert!(!background.has_room());
        assert!(background.start(control()).is_none());
    }
}
