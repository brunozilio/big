use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;

use domain::effect::{Effect, EffectOutcome, EffectRequest, IdempotencyKey};
use domain::workflow::{ItemId, StepState, WorkItem, WorkflowEvent};
use serde_json::{Value, json};
use tokio::sync::{OwnedSemaphorePermit, mpsc};
use tokio::task::AbortHandle;

use crate::activation::Script;
use crate::children;
use crate::effects::{Launch, launch};
use crate::error::{Error, Result};
use crate::events::{OrchestratorEvent, Wake};
use crate::ledger::Ledger;
use crate::protocol::{AGENT, Activated, CHILD, Command, Signal, WAIT_HUMAN, kind_of};
use crate::rewind;
use crate::shared::Shared;
use crate::views::item_json;

struct Inflight {
    key: IdempotencyKey,
    tasks: Vec<AbortHandle>,
    works: bool,
}

pub struct Runner {
    shared: Arc<Shared>,
    id: ItemId,
    generation: u64,
    wakes: mpsc::UnboundedReceiver<Wake>,
    inflight: HashMap<u64, Inflight>,
    blocked: BTreeSet<u64>,
    permit: Option<OwnedSemaphorePermit>,
    realigned: BTreeSet<String>,
}

pub fn ensure(shared: &Arc<Shared>, item: &ItemId) {
    if let Some((generation, wakes)) = shared.register(item) {
        let runner = Runner {
            shared: Arc::clone(shared),
            id: item.clone(),
            generation,
            wakes,
            inflight: HashMap::new(),
            blocked: BTreeSet::new(),
            permit: None,
            realigned: BTreeSet::new(),
        };
        shared.spawn(runner.run());
    } else {
        shared.wake(item, Wake::Poke);
    }
}

fn mark_waiting(shared: &Shared, item: &WorkItem, step: Option<(&str, &str)>) -> Result<()> {
    let Some((step, branch)) = step else { return Ok(()) };
    let Some(run) = item.step(step, Some(branch)) else { return Ok(()) };
    if run.state == StepState::WaitingHuman {
        return Ok(());
    }
    let finished = WorkflowEvent::StepFinished {
        step: step.to_owned(),
        branch: Some(branch.to_owned()),
        state: StepState::WaitingHuman,
        output: run.output.clone(),
    };
    shared.append(&item.id, &[finished]).map(|_| ())
}

fn impede(shared: &Shared, item: &WorkItem, reason: &str) -> Result<()> {
    if item.impediment.as_ref().is_some_and(|found| found.reason.as_deref() == Some(reason)) {
        return Ok(());
    }
    shared.append(&item.id, &[WorkflowEvent::ImpedimentSet { on: true, reason: Some(reason.to_owned()) }]).map(|_| ())
}

pub fn block(shared: &Shared, item: &ItemId, key: &IdempotencyKey, reason: &str) -> Result<()> {
    let state = shared.item(item)?;
    let timeout = crate::shared::timeout_timer(key);
    if state.timers.contains_key(&timeout) {
        shared.append(item, &[WorkflowEvent::TimerFired { id: timeout }])?;
    }
    impede(shared, &state, reason)?;
    let branch = key.branch.clone().unwrap_or_default();
    mark_waiting(shared, &shared.item(item)?, Some((&key.step, &branch)))
}

fn park(shared: &Shared, item: &ItemId, ledger: &Ledger, reason: &str) -> Result<()> {
    let state = shared.item(item)?;
    impede(shared, &state, reason)?;
    let last = ledger.last_step();
    mark_waiting(shared, &shared.item(item)?, last.as_ref().map(|(step, branch)| (step.as_str(), branch.as_str())))
}

fn signal_events(signal: &Signal) -> Vec<WorkflowEvent> {
    let data = Value::Object(signal.data.clone());
    let mut events = vec![WorkflowEvent::Signal {
        seq: signal.seq,
        kind: signal.kind.clone(),
        branch: signal.branch.clone(),
        step: signal.step.clone(),
        attempt: signal.attempt,
        data: data.clone(),
    }];
    let Some(step) = signal.step.clone() else { return events };
    let branch = Some(signal.branch.clone());
    match signal.kind.as_str() {
        "step.started" => events.push(WorkflowEvent::StepStarted { step, branch }),
        "step.finished" => {
            let state = match data["outcome"].as_str() {
                Some("failed" | "goto") => StepState::Failed,
                Some("cancelled" | "skipped") => StepState::Skipped,
                _ => StepState::Passed,
            };
            events.push(WorkflowEvent::StepFinished { step, branch, state, output: data["data"].clone() });
        }
        _ => {}
    }
    events
}

impl Runner {
    async fn run(mut self) {
        loop {
            match self.turn().await {
                Ok(true) => {}
                Ok(false) => break,
                Err(error) => {
                    tracing::warn!(%error, item = %self.id, "an activation failed");
                    if let Ok(ledger) = Ledger::load(&self.shared.store, &self.id) {
                        let _ = park(&self.shared, &self.id, &ledger, &error.to_string());
                    }
                }
            }
            let Some(wake) = self.wakes.recv().await else { break };
            self.take(wake);
            while let Ok(wake) = self.wakes.try_recv() {
                self.take(wake);
            }
        }
        self.abort_all();
        self.shared.unregister(&self.id, self.generation);
    }

    fn take(&mut self, wake: Wake) {
        match wake {
            Wake::Poke => {}
            Wake::Blocked(seq) => {
                if let Some(known) = self.inflight.get(&seq) {
                    known.tasks.iter().for_each(AbortHandle::abort);
                }
                self.blocked.insert(seq);
            }
            Wake::Rerun => {
                for seq in std::mem::take(&mut self.blocked) {
                    self.drop_inflight(seq);
                }
            }
        }
    }

    fn drop_inflight(&mut self, seq: u64) {
        if let Some(gone) = self.inflight.remove(&seq) {
            gone.tasks.iter().for_each(AbortHandle::abort);
            self.shared.forget_waits(&self.id, Some(&gone.key));
            self.shared.asks.forget(&gone.key);
        }
        if !self.inflight.values().any(|known| known.works) {
            self.permit = None;
        }
    }

    fn abort_all(&mut self) {
        let seqs: Vec<u64> = self.inflight.keys().copied().collect();
        seqs.into_iter().for_each(|seq| self.drop_inflight(seq));
        self.shared.forget_waits(&self.id, None);
    }

    async fn turn(&mut self) -> Result<bool> {
        let item = self.shared.item(&self.id)?;
        if item.is_ended() {
            self.close(&item)?;
            return Ok(false);
        }
        children::sweep(&self.shared, &self.id, &Ledger::load(&self.shared.store, &self.id)?)?;
        let ledger = Ledger::load(&self.shared.store, &self.id)?;
        self.settle(&ledger);
        let activated = match self.activate(&item, &ledger).await {
            Ok(activated) => activated,
            Err(Error::Script(reason)) => {
                let text = crate::texts::not_loaded(crate::texts::of_item(&self.shared, &self.id), &reason);
                park(&self.shared, &self.id, &ledger, &text)?;
                return Ok(true);
            }
            Err(other) => return Err(other),
        };
        if activated.state != "nondeterministic" {
            self.record(&ledger, &activated.signals)?;
            self.cancel(&ledger, &activated.cancel)?;
        }
        match activated.state.as_str() {
            "running" => {
                self.start(&ledger, activated.pending).await?;
                Ok(true)
            }
            "completed" | "cancelled" => {
                let ending = if activated.state == "completed" {
                    WorkflowEvent::Finished { output: activated.result }
                } else {
                    WorkflowEvent::Cancelled { reason: activated.reason.unwrap_or_default() }
                };
                let ended = self.shared.append(&self.id, &[ending])?;
                self.close(&ended)?;
                Ok(false)
            }
            "nondeterministic" => self.realign(&ledger, &activated.error).await,
            _ => {
                let message = activated.error["message"].as_str().unwrap_or_default();
                let text = crate::texts::failed(crate::texts::of_item(&self.shared, &self.id), message);
                park(&self.shared, &self.id, &ledger, &text)?;
                Ok(true)
            }
        }
    }

    async fn activate(&self, item: &WorkItem, ledger: &Ledger) -> Result<Activated> {
        let shared = Arc::clone(&self.shared);
        let owner = item.clone();
        let mut activation = json!({
            "item": item_json(item),
            "journal": ledger.journal(),
            "start": ledger.start,
            "seed": item.id.as_str(),
        });
        tokio::task::spawn_blocking(move || {
            let script = shared.script_for(&owner)?;
            run_script(&script, &mut activation)
        })
        .await
        .map_err(|error| Error::Script(error.to_string()))?
    }

    async fn realign(&mut self, ledger: &Ledger, error: &Value) -> Result<bool> {
        let diverged = error["seq"].as_u64().and_then(|seq| ledger.requested.get(&seq));
        let found = diverged.and_then(|request| {
            let step = request.key.step.clone();
            ledger.cut_before(&step, request.key.seq).map(|cut| (step, cut))
        });
        if let Some((step, cut)) = found.filter(|(step, _)| self.realigned.insert(step.clone())) {
            tracing::info!(item = %self.id, %step, "the workflow code changed; the step runs again with it");
            rewind::rewind_at(&self.shared, &self.id, ledger, &step, cut).await?;
            return Ok(true);
        }
        let message = error["message"].as_str().unwrap_or_default();
        let reason = crate::texts::workflow_changed(crate::texts::of_item(&self.shared, &self.id), message);
        park(&self.shared, &self.id, ledger, &reason)?;
        Ok(true)
    }

    fn settle(&mut self, ledger: &Ledger) {
        let done: Vec<u64> = self
            .inflight
            .iter()
            .filter(|(seq, known)| {
                ledger.resolved.contains_key(seq)
                    || ledger.requested.get(seq).is_none_or(|request| request.key != known.key)
            })
            .map(|(seq, _)| *seq)
            .collect();
        done.into_iter().for_each(|seq| self.drop_inflight(seq));
    }

    fn record(&self, ledger: &Ledger, signals: &[Signal]) -> Result<()> {
        let events: Vec<WorkflowEvent> =
            signals.iter().filter(|signal| !ledger.signals.contains_key(&signal.seq)).flat_map(signal_events).collect();
        if !events.is_empty() {
            self.shared.append(&self.id, &events)?;
        }
        Ok(())
    }

    fn cancel(&mut self, ledger: &Ledger, cancel: &[u64]) -> Result<()> {
        for seq in cancel {
            self.drop_inflight(*seq);
            let Some(request) = ledger.requested.get(seq).filter(|_| !ledger.resolved.contains_key(seq)) else {
                continue;
            };
            if kind_of(&request.effect) == CHILD {
                crate::api::cancel_item(
                    &self.shared,
                    &children::child_id(&self.id, &request.key),
                    crate::texts::parent_stopped_waiting(crate::texts::of_item(&self.shared, &self.id)),
                )?;
            }
            self.shared.resolve(&self.id, &request.key, EffectOutcome::Cancelled)?;
        }
        Ok(())
    }

    fn key_for(&self, item: &WorkItem, ledger: &Ledger, command: &Command) -> IdempotencyKey {
        let step = command.step.clone().unwrap_or_default();
        let attempt = item
            .step(&step, Some(&command.branch))
            .filter(|_| command.step.is_some())
            .map_or(ledger.rewinds, |run| run.attempt);
        IdempotencyKey::new(self.id.as_str(), &step, Some(&command.branch), command.seq, attempt)
    }

    async fn start(&mut self, ledger: &Ledger, pending: Vec<Command>) -> Result<()> {
        let item = self.shared.item(&self.id)?;
        let fresh: Vec<Command> =
            pending.into_iter().filter(|command| !self.inflight.contains_key(&command.seq)).collect();
        let mut requests = Vec::new();
        let mut launches = Vec::new();
        for mut command in fresh {
            let key = match ledger.requested.get(&command.seq) {
                Some(known) => {
                    if let Effect::AgentRun { input, .. } = &known.effect {
                        command.args.clone_from(input);
                    }
                    if let Some(refreshed) = refreshed(ledger, known, &command) {
                        requests.push(WorkflowEvent::EffectRequested(refreshed));
                    }
                    known.key.clone()
                }
                None => {
                    let key = self.key_for(&item, ledger, &command);
                    let mut request = EffectRequest::new(key.clone(), command.effect())
                        .with_retry(command.retry(&self.shared.config.limits.retry));
                    request.timeout = command.timeout.map(std::time::Duration::from_millis);
                    requests.push(WorkflowEvent::EffectRequested(request));
                    key
                }
            };
            let resume = (command.kind == AGENT)
                .then(|| ledger.progress.get(&command.seq).and_then(|data| data["session"].as_str().map(str::to_owned)))
                .flatten();
            launches.push(Launch { command, key, resume });
        }
        if !requests.is_empty() {
            self.shared.append(&self.id, &requests)?;
        }
        for launched in launches {
            let works = launched.command.works();
            if works && self.permit.is_none() {
                let permits = Arc::clone(&self.shared.permits);
                self.permit = permits.acquire_owned().await.ok();
            }
            let (seq, key) = (launched.command.seq, launched.key.clone());
            let tasks = launch(&self.shared, &self.id, launched)?;
            self.inflight.insert(seq, Inflight { key, tasks, works });
        }
        Ok(())
    }

    fn close(&mut self, item: &WorkItem) -> Result<()> {
        self.abort_all();
        for child in item.children.iter().filter(|child| !child.finished) {
            crate::api::cancel_item(
                &self.shared,
                &child.id,
                crate::texts::parent_finished(crate::texts::of_item(&self.shared, &item.id)),
            )?;
        }
        crate::release::release(&self.shared, item);
        self.shared.emit(OrchestratorEvent::ItemEnded { item: item.id.clone() });
        if let Some(parent) = &item.parent {
            self.shared.wake(parent, Wake::Poke);
        }
        Ok(())
    }
}

fn run_script(script: &Script, activation: &mut Value) -> Result<Activated> {
    activation["target"] = script.target.clone();
    let value = script.activate(activation).map_err(Error::Script)?;
    serde_json::from_value(value).map_err(|error| Error::Script(error.to_string()))
}

fn refreshed(ledger: &Ledger, known: &EffectRequest, command: &Command) -> Option<EffectRequest> {
    let open = !ledger.resolved.contains_key(&command.seq);
    let effect = command.effect();
    let changed =
        open && kind_of(&known.effect) == WAIT_HUMAN && kind_of(&effect) == WAIT_HUMAN && known.effect != effect;
    changed.then(|| EffectRequest { effect, ..known.clone() })
}
