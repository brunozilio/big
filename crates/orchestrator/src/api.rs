use std::ops::Deref;
use std::path::PathBuf;
use std::sync::{Arc, PoisonError};

use connectors::ExternalEvent;
use domain::effect::EffectOutcome;
use domain::workflow::{ItemId, WorkItem, WorkflowEvent};
use domain::{AskId, Decision};
use serde_json::{Value, json};
use sync::{Health, ReconcilePolicy, StoreView, SyncConfig, Synchronizer};
use tokio::sync::broadcast;

use crate::agent::rewind_session;
use crate::asks::PendingAsk;
use crate::config::Config;
use crate::error::{Error, Result};
use crate::events::{OrchestratorEvent, Wake};
use crate::git;
use crate::ledger::Ledger;
use crate::matching::event_matches;
use crate::protocol::{AGENT, WAIT_HUMAN, kind_of};
use crate::runner::ensure;
use crate::shared::Shared;
use crate::sink::KernelSink;
use crate::views::{ItemSummary, StepView, attempts, ends, live_blocks, summary, view_state};

#[derive(Debug, Clone)]
pub struct Orchestrator {
    pub(crate) shared: Arc<Shared>,
}

#[derive(Debug)]
pub struct Handle {
    orchestrator: Orchestrator,
}

impl Deref for Handle {
    type Target = Orchestrator;

    fn deref(&self) -> &Orchestrator {
        &self.orchestrator
    }
}

impl Drop for Handle {
    fn drop(&mut self) {
        self.orchestrator.shutdown();
    }
}

pub fn cancel_item(shared: &Arc<Shared>, id: &ItemId, reason: &str) -> Result<()> {
    let item = shared.item(id)?;
    if item.is_ended() {
        return Ok(());
    }
    let ended = shared.append(id, &[WorkflowEvent::Cancelled { reason: reason.to_owned() }])?;
    if !shared.wake(id, Wake::Poke) {
        for child in ended.children.iter().filter(|child| !child.finished) {
            cancel_item(shared, &child.id, reason)?;
        }
        if let Some(parent) = &ended.parent {
            shared.wake(parent, Wake::Poke);
        }
        shared.emit(OrchestratorEvent::ItemEnded { item: id.clone() });
    }
    Ok(())
}

impl Orchestrator {
    pub fn start(config: Config) -> Result<Handle> {
        let syncing = config.sync;
        let shared = Arc::new(Shared::new(config));
        if syncing {
            start_sync(&shared);
        }
        for item in shared.store.items()?.into_iter().filter(|item| !item.is_ended()) {
            ensure(&shared, &item.id);
        }
        Ok(Handle { orchestrator: Orchestrator { shared } })
    }

    pub fn shutdown(&self) {
        self.shared.token.cancel();
        if let Some(sync) = self.shared.sync.get() {
            sync.stop();
        }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<OrchestratorEvent> {
        self.shared.events.subscribe()
    }

    pub fn health(&self) -> Option<Arc<Health>> {
        self.shared.sync.get().map(|sync| Arc::clone(sync.health()))
    }

    pub fn workflows(&self) -> Vec<String> {
        self.shared.config.resources.workflows()
    }

    pub fn create_item(&self, workflow: &str, input: Value) -> Result<ItemId> {
        self.create(workflow, input, None)
    }

    fn create(&self, workflow: &str, input: Value, parent: Option<ItemId>) -> Result<ItemId> {
        if self.shared.config.resources.workflow(workflow).is_none() {
            return Err(Error::Script(format!("there is no workflow named {workflow}")));
        }
        let id = ItemId::new();
        let created = WorkflowEvent::ItemCreated { id: id.clone(), workflow: workflow.to_owned(), parent, input };
        self.shared.append(&id, &[created])?;
        ensure(&self.shared, &id);
        Ok(id)
    }

    pub fn item(&self, id: &ItemId) -> Result<Option<WorkItem>> {
        Ok(self.shared.store.item(id)?)
    }

    pub fn items(&self) -> Result<Vec<ItemSummary>> {
        let items = self.shared.store.items()?;
        items.iter().map(|item| self.summarize(item)).collect()
    }

    pub fn summary(&self, id: &ItemId) -> Result<Option<ItemSummary>> {
        self.shared.store.item(id)?.map(|item| self.summarize(&item)).transpose()
    }

    fn summarize(&self, item: &WorkItem) -> Result<ItemSummary> {
        let ledger = Ledger::load(&self.shared.store, &item.id)?;
        let times: Vec<(i64, bool)> =
            self.shared.store.timed_events(&item.id, 0)?.iter().map(|(_, at, event)| (*at, ends(event))).collect();
        let declared = self.declared(&item.workflow);
        Ok(summary(item, &ledger, &times, self.shared.asks.count(&item.id), &declared))
    }

    fn declared(&self, workflow: &str) -> Vec<(String, Option<String>)> {
        let Ok(script) = self.shared.script(workflow) else { return Vec::new() };
        let chosen = script.target["workflow"].clone();
        let shapes = script.described["workflows"].as_array().cloned().unwrap_or_default();
        let shape = shapes.into_iter().find(|shape| shape["name"] == chosen).unwrap_or(Value::Null);
        let steps = shape["steps"].as_array().cloned().unwrap_or_default();
        steps
            .iter()
            .filter_map(|call| Some((call["id"].as_str()?.to_owned(), call["step"].as_str().map(str::to_owned))))
            .collect()
    }

    pub fn describe(&self, workflow: &str) -> Value {
        self.shared.script(workflow).map(|script| script.described.clone()).unwrap_or(Value::Null)
    }

    pub async fn step_view(&self, id: &ItemId, step: &str, attempt: Option<u32>) -> Result<StepView> {
        let item = self.shared.item(id)?;
        let ledger = Ledger::load(&self.shared.store, id)?;
        let tried = attempts(&ledger, step);
        let latest = tried.last().map_or(0, |last| last.n);
        let chosen = attempt.and_then(|wanted| tried.iter().find(|tried| tried.n == wanted && wanted != latest));
        let kind = ledger.step_type(step);
        let live = if chosen.is_none() { live_blocks(&ledger, step) } else { None };
        let blocks = match (live, kind.clone()) {
            (Some(blocks), _) => blocks,
            (None, Some(kind)) => {
                let request = json!({ "step": kind, "state": view_state(&item, &ledger, step, chosen) });
                let shared = Arc::clone(&self.shared);
                let workflow = item.workflow.clone();
                tokio::task::spawn_blocking(move || shared.script(&workflow)?.view(&request).map_err(Error::Script))
                    .await
                    .map_err(|error| Error::Script(error.to_string()))??
            }
            (None, None) => json!([]),
        };
        Ok(StepView {
            item: id.clone(),
            step: step.to_owned(),
            kind,
            attempt: chosen.map_or(latest, |chosen| chosen.n),
            attempts: tried.clone(),
            session: crate::views::session_of(&ledger, step),
            blocks,
        })
    }

    pub fn human_input(&self, id: &ItemId, step: &str, action: &str, payload: Value) -> Result<()> {
        let ledger = Ledger::load(&self.shared.store, id)?;
        let waits: Vec<_> = ledger.open().filter(|request| kind_of(&request.effect) == WAIT_HUMAN).collect();
        let only = if let [only] = waits.as_slice() { Some(only) } else { None };
        let found = waits.iter().find(|request| request.key.step == step).or(only);
        let request = found.ok_or_else(|| Error::NotWaiting { item: id.to_string(), step: step.to_owned() })?;
        let answer = WorkflowEvent::HumanInput {
            step: request.key.step.clone(),
            branch: request.key.branch.clone(),
            action: action.to_owned(),
            payload: payload.clone(),
        };
        self.shared.append(id, &[answer])?;
        let value = json!({ "action": action, "values": payload, "by": "human" });
        self.shared.resolve(id, &request.key, EffectOutcome::Ok { value })?;
        Ok(())
    }

    pub fn cancel(&self, id: &ItemId) -> Result<()> {
        cancel_item(&self.shared, id, "cancelado por quem acompanha o item")
    }

    pub async fn rerun(&self, id: &ItemId) -> Result<()> {
        let item = self.shared.item(id)?;
        self.shared.forget_script(&item.workflow);
        if item.is_ended() {
            let first = item.steps.first().map(|run| run.id.clone()).ok_or_else(|| Error::Ended(id.to_string()))?;
            return self.rewind(id, &first).await;
        }
        if item.impediment.is_some() {
            self.shared.append(id, &[WorkflowEvent::ImpedimentSet { on: false, reason: None }])?;
        }
        ensure(&self.shared, id);
        self.shared.wake(id, Wake::Rerun);
        Ok(())
    }

    pub async fn rewind(&self, id: &ItemId, step: &str) -> Result<()> {
        let ledger = Ledger::load(&self.shared.store, id)?;
        let cut = ledger.cut(step).ok_or_else(|| Error::UnknownStep(step.to_owned()))?;
        let first = ledger
            .requested
            .range(cut..)
            .filter(|(_, request)| kind_of(&request.effect) == AGENT)
            .find_map(|(seq, _)| ledger.value(*seq).cloned());
        if let Some(agent) = first {
            self.restore(&agent).await;
        }
        self.shared.append(id, &[WorkflowEvent::Rewound { to: step.to_owned() }])?;
        ensure(&self.shared, id);
        self.shared.wake(id, Wake::Rerun);
        Ok(())
    }

    async fn restore(&self, agent: &Value) {
        let Some(worktree) = agent["worktree"].as_str().map(PathBuf::from) else { return };
        if let Some(base) = agent["base"].as_str().map(str::to_owned) {
            let place = worktree.clone();
            let _ = tokio::task::spawn_blocking(move || git::restore(&place, &base)).await;
        }
        let session = agent["session"].as_str();
        if let (Some(session), Some(checkpoint)) = (session, agent["checkpoint"].as_u64()) {
            let checkpoint = u32::try_from(checkpoint).unwrap_or(u32::MAX);
            rewind_session(&self.shared, worktree, session, checkpoint).await;
        }
    }

    pub fn asks(&self) -> Vec<PendingAsk> {
        self.shared.asks.list()
    }

    pub fn answer_ask(&self, ask: &AskId, decision: &Decision) -> Result<()> {
        let answered = self.shared.asks.answer(ask, decision).ok_or_else(|| Error::UnknownAsk(ask.0.clone()))?;
        self.shared.emit(OrchestratorEvent::AskClosed { item: answered.item, ask: ask.clone() });
        Ok(())
    }

    pub fn deliver_event(&self, event: &ExternalEvent) -> Result<usize> {
        let value = serde_json::to_value(event).unwrap_or(Value::Null);
        let matched = {
            let mut waits = self.shared.waits.lock().unwrap_or_else(PoisonError::into_inner);
            let (hit, kept): (Vec<_>, Vec<_>) =
                waits.drain(..).partition(|wait| wait.kind == event.kind && event_matches(&wait.filter, &value));
            *waits = kept;
            hit
        };
        let mut woken: Vec<ItemId> = Vec::new();
        for wait in matched {
            if self.shared.resolve(&wait.item, &wait.key, EffectOutcome::Ok { value: value.clone() })?
                && !woken.contains(&wait.item)
            {
                woken.push(wait.item);
            }
        }
        Ok(woken.len())
    }

    pub fn trigger(&self, event: &ExternalEvent) -> Result<usize> {
        let value = serde_json::to_value(event).unwrap_or(Value::Null);
        let existing = self.shared.store.items()?;
        let mut created = 0;
        for workflow in self.workflows() {
            let Ok(script) = self.shared.script(&workflow) else { continue };
            let shapes = script.described["workflows"].as_array().cloned().unwrap_or_default();
            let Some(shape) = shapes.iter().find(|shape| shape["name"] == script.target["workflow"]) else { continue };
            let triggers = shape["triggers"].as_array().cloned().unwrap_or_default();
            let fires =
                triggers.iter().any(|on| on["kind"] == event.kind.as_str() && event_matches(&on["filter"], &value));
            let known =
                existing.iter().any(|item| item.workflow == workflow && item.input["issue"] == event.subject.as_str());
            if fires && !known {
                let title = value["data"]["title"].as_str().unwrap_or(&event.subject).to_owned();
                let input =
                    json!({ "title": title, "issue": event.subject, "connection": event.connection, "event": value });
                self.create(&workflow, input, None)?;
                created += 1;
            }
        }
        Ok(created)
    }

    pub fn record(&self, id: &ItemId, events: &[WorkflowEvent]) -> Result<()> {
        self.shared.append(id, events)?;
        self.shared.wake(id, Wake::Poke);
        Ok(())
    }
}

fn start_sync(shared: &Arc<Shared>) {
    let view = Arc::new(StoreView::new(Arc::clone(&shared.store)));
    for workflow in shared.config.resources.workflows() {
        let Ok(script) = shared.script(&workflow) else { continue };
        for shape in script.described["workflows"].as_array().cloned().unwrap_or_default() {
            let policy = ReconcilePolicy::from_workflow(&shape["reconcile"]);
            if shape["name"] == script.target["workflow"] {
                view.set_policy(&workflow, policy);
            }
            if let Some(name) = shape["name"].as_str() {
                view.set_policy(name, policy);
            }
        }
    }
    let sink = Arc::new(KernelSink::new(Arc::downgrade(shared)));
    let synchronizer = Arc::new(Synchronizer::new(
        Arc::clone(&shared.store),
        Arc::clone(&shared.config.connectors),
        sink,
        view,
        SyncConfig::from_brand(),
    ));
    if shared.sync.set(Arc::clone(&synchronizer)).is_ok() {
        shared.spawn(async move { synchronizer.run().await });
    }
}

impl Orchestrator {
    pub(crate) fn of(shared: Arc<Shared>) -> Orchestrator {
        Orchestrator { shared }
    }
}
