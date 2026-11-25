use std::ops::Deref;
use std::path::PathBuf;
use std::sync::{Arc, PoisonError};

use connectors::ExternalEvent;
use domain::effect::EffectOutcome;
use domain::workflow::{ItemId, StepState, WorkItem, WorkflowEvent};
use domain::{AskId, Decision};
use serde_json::{Value, json};
use sync::{Health, ReconcilePolicy, StoreView, Synchronizer};
use tokio::sync::broadcast;

use crate::asks::PendingAsk;
use crate::cards::AgentCard;
use crate::config::Config;
use crate::error::{Error, Result};
use crate::events::{OrchestratorEvent, Wake};
use crate::ledger::Ledger;
use crate::matching::event_matches;
use crate::protocol::{WAIT_HUMAN, kind_of};
use crate::provenance;
use crate::resources::Origin;
use crate::rewind;
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
        crate::release::release(shared, &ended);
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
        crate::asks::restore(&shared)?;
        crate::factory::watch(&shared);
        crate::progress::watch(&shared);
        crate::schedules::watch(&shared);
        for item in shared.store.active_items()? {
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

    pub fn watch(&self, watch: crate::config::Watch) {
        let _ = self.shared.config.watch.set(watch);
    }

    pub fn subscribe(&self) -> broadcast::Receiver<OrchestratorEvent> {
        self.shared.events.subscribe()
    }

    pub fn health(&self) -> Option<Arc<Health>> {
        self.shared.sync.get().map(|sync| Arc::clone(sync.health()))
    }

    pub fn directory(&self) -> Option<PathBuf> {
        self.shared.config.repo.clone()
    }

    pub async fn cloud_credentials(&self, token: &str) -> Result<Value> {
        crate::cloud::credentials(&self.shared, token).await
    }

    pub fn workflows(&self) -> Vec<String> {
        self.shared.config.resources.workflows()
    }

    pub fn create_item(&self, workflow: &str, input: Value) -> Result<ItemId> {
        self.create_with(workflow, input, None)
    }

    pub(crate) fn create_with(&self, workflow: &str, mut input: Value, parent: Option<ItemId>) -> Result<ItemId> {
        let resources = self.shared.resources_in(self.shared.home(&input).as_deref());
        if resources.workflow(workflow).is_none() {
            return Err(Error::Script(format!("there is no workflow named {workflow}")));
        }
        if parent.is_none() {
            crate::factory::stamp(&self.shared, &resources, &mut input)?;
        }
        self.stamp_language(&mut input, parent.as_ref())?;
        let id = ItemId::new();
        let named = parent.is_none() && !crate::naming::titled(&input);
        let text = input["text"].as_str().or(input["title"].as_str()).unwrap_or_default().to_owned();
        let wanted = input["model"].as_str().map(str::to_owned);
        let home = self.shared.home(&input).unwrap_or_else(|| self.shared.config.data.clone());
        let created = WorkflowEvent::ItemCreated { id: id.clone(), workflow: workflow.to_owned(), parent, input };
        self.shared.append(&id, &[created])?;
        ensure(&self.shared, &id);
        if named && !text.trim().is_empty() {
            crate::naming::name(&self.shared, id.clone(), text, home, wanted);
        }
        Ok(id)
    }

    fn stamp_language(&self, input: &mut Value, parent: Option<&ItemId>) -> Result<()> {
        let Some(fields) = input.as_object_mut() else { return Ok(()) };
        if fields.get("language").and_then(Value::as_str).is_some() {
            return Ok(());
        }
        let inherited = match parent {
            Some(parent) => self.shared.store.item(parent)?.map(|item| item.input["language"].clone()),
            None => None,
        };
        let language = match inherited.filter(Value::is_string) {
            Some(language) => language,
            None => json!(self.shared.store.language()?.code()),
        };
        fields.insert("language".to_owned(), language);
        Ok(())
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
        let declared = self.declared(item);
        let mut found = summary(item, &ledger, &times, self.shared.asks.count(&item.id), &declared);
        found.plans = self.shared.asks.plans(&item.id);
        for step in found.steps.iter_mut().filter(|step| step.state == StepState::Running) {
            if self.shared.asks.asking(&item.id, &step.id) {
                step.state = StepState::WaitingHuman;
            }
        }
        Ok(found)
    }

    fn declared(&self, item: &WorkItem) -> Vec<(String, Option<String>)> {
        let Ok(script) = self.shared.script_for(item) else { return Vec::new() };
        let chosen = script.target["workflow"].clone();
        let shapes = script.described["workflows"].as_array().cloned().unwrap_or_default();
        let shape = shapes.into_iter().find(|shape| shape["name"] == chosen).unwrap_or(Value::Null);
        let mut found = Vec::new();
        flatten(&shape["steps"], &mut found);
        found
    }

    pub fn describe(&self, workflow: &str) -> Value {
        self.shared.script(workflow).map(|script| script.described.clone()).unwrap_or(Value::Null)
    }

    pub async fn step_view(&self, id: &ItemId, step: &str, attempt: Option<u32>) -> Result<StepView> {
        let item = self.shared.item(id)?;
        let ledger = Ledger::load(&self.shared.store, id)?;
        let mut tried = attempts(&ledger, step);
        let held = item.steps.iter().any(|run| run.id == step && run.state == StepState::WaitingHuman);
        if let Some(last) = tried.last_mut().filter(|last| last.state == StepState::Running)
            && (held || self.shared.asks.asking(id, step))
        {
            last.state = StepState::WaitingHuman;
        }
        let latest = tried.last().map_or(0, |last| last.n);
        let chosen = attempt.and_then(|wanted| tried.iter().find(|tried| tried.n == wanted && wanted != latest));
        let kind = ledger.step_type(step);
        let live = if chosen.is_none() { live_blocks(&ledger, step) } else { None };
        let blocks = match (live, kind.clone()) {
            (Some(blocks), _) => blocks,
            (None, Some(kind)) => {
                let request = json!({ "step": kind, "state": view_state(&item, &ledger, step, chosen) });
                let shared = Arc::clone(&self.shared);
                let owner = item.clone();
                tokio::task::spawn_blocking(move || shared.script_for(&owner)?.view(&request).map_err(Error::Script))
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
            session: crate::views::session_of(&ledger, step, chosen.map(|chosen| chosen.n)),
            heads_up: item.heads_up.get(step).cloned(),
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
        cancel_item(&self.shared, id, crate::texts::cancelled_by_watcher(crate::texts::of_item(&self.shared, id)))
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
        rewind::rewind(&self.shared, id, step).await
    }

    pub fn item_sessions(&self, id: &ItemId) -> Result<Vec<crate::ItemSession>> {
        Ok(self.shared.store.item(id)?.map(|item| crate::context::sessions(&item)).unwrap_or_default())
    }

    pub fn context_window(&self, id: &ItemId, model: Option<&str>) -> Result<u64> {
        let item = self.shared.item(id)?;
        Ok(crate::context::window(&self.shared, &item, model))
    }

    pub fn session_agents(&self, session: &str) -> Result<Vec<AgentCard>> {
        let items = self.shared.store.items()?;
        let names = crate::cards::agents_of(&items, session);
        Ok(names.iter().map(|name| crate::cards::card(&self.shared.config.resources, name)).collect())
    }

    pub fn session_agent(&self, session: &str) -> Result<Option<domain::MainAgentSpec>> {
        let items = self.shared.store.items()?;
        let owner = items.iter().find_map(|item| {
            crate::cards::agents_of(std::slice::from_ref(item), session).into_iter().next().map(|name| (item, name))
        });
        let Some((item, name)) = owner else { return Ok(None) };
        let resources = self.shared.resources_in(self.shared.home(&item.input).as_deref());
        Ok(resources.agent(&name).map(|definition| domain::MainAgentSpec { name, definition: Some(definition.text) }))
    }

    pub fn asks(&self) -> Vec<PendingAsk> {
        self.shared.asks.list()
    }

    pub fn answer_ask(&self, ask: &AskId, decision: &Decision) -> Result<()> {
        self.answer_ask_with(ask, decision, Value::Null)
    }

    pub fn answer_ask_with(&self, ask: &AskId, decision: &Decision, note: Value) -> Result<()> {
        let answered = self.shared.asks.answer(ask, decision).ok_or_else(|| Error::UnknownAsk(ask.0.clone()))?;
        let pending = &answered.pending;
        let record = json!({
            "ask": pending.ask, "session": pending.session, "decision": decision, "note": note,
            "at": crate::shared::now_ms(),
        });
        let data = json!({ "asks": { ask.0.clone(): null }, "answered": { ask.0.clone(): record } });
        self.shared.append(&pending.item, &[WorkflowEvent::EffectProgress { key: answered.key.clone(), data }])?;
        self.shared.emit(OrchestratorEvent::AskClosed { item: pending.item.clone(), ask: ask.clone() });
        Ok(())
    }

    pub fn answered(&self, session: &str) -> Result<Vec<Value>> {
        let items = self.shared.store.items()?;
        let mut found: Vec<Value> = items
            .iter()
            .flat_map(|item| item.effects.values())
            .filter_map(|entry| entry.progress["answered"].as_object())
            .flat_map(|answered| answered.values())
            .filter(|record| record["session"] == session)
            .map(|record| record["note"].clone())
            .filter(|note| !note.is_null())
            .collect();
        found.sort_by(|left, right| left["at"].as_str().cmp(&right["at"].as_str()));
        Ok(found)
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
        if provenance::owned(&existing, &event.subject) {
            return Ok(0);
        }
        let fresh: Vec<String> = self
            .triggered(&event.kind, &value)
            .into_iter()
            .filter(|workflow| {
                !existing.iter().any(|item| &item.workflow == workflow && mentions(&item.input, &event.subject))
            })
            .collect();
        let resources = &self.shared.config.resources;
        let custom =
            |workflow: &&String| resources.workflow(workflow).is_some_and(|found| found.origin != Origin::Builtin);
        let Some(workflow) = fresh.iter().find(custom).or(fresh.first()) else { return Ok(0) };
        let kind = source_kind(event.kind.split('.').next().unwrap_or(event.capability.as_str()));
        let title = value["data"]["title"].as_str().unwrap_or(&event.subject).to_owned();
        let source = json!({ "kind": kind, "connection": event.connection, "subject": event.subject });
        let mut input = json!({ "title": title, "sources": [source], "event": value });
        if kind == ISSUE {
            input["issue"] = json!(event.subject);
            input["connection"] = json!(event.connection);
        }
        provenance::attach(&self.shared.store, &mut input, &value);
        self.create_with(workflow, input, None)?;
        Ok(1)
    }

    pub(crate) fn shape(&self, workflow: &str) -> Option<Value> {
        let script = self.shared.script(workflow).ok()?;
        let shapes = script.described["workflows"].as_array().cloned().unwrap_or_default();
        shapes.into_iter().find(|shape| shape["name"] == script.target["workflow"])
    }

    pub fn accepts(&self, workflow: &str) -> Vec<String> {
        let Some(shape) = self.shape(workflow) else { return Vec::new() };
        let listed = shape["accepts"].as_array().cloned().unwrap_or_default();
        listed.iter().filter_map(Value::as_str).map(str::to_owned).collect()
    }

    pub(crate) fn triggered(&self, kind: &str, event: &Value) -> Vec<String> {
        let mut fired = Vec::new();
        for workflow in self.workflows() {
            let Some(shape) = self.shape(&workflow) else { continue };
            let triggers = shape["triggers"].as_array().cloned().unwrap_or_default();
            if triggers.iter().any(|on| on["kind"] == kind && event_matches(&on["filter"], event)) {
                fired.push(workflow);
            }
        }
        fired
    }

    pub fn record(&self, id: &ItemId, events: &[WorkflowEvent]) -> Result<()> {
        self.shared.append(id, events)?;
        self.shared.wake(id, Wake::Poke);
        Ok(())
    }
}

const ISSUE: &str = "issue";

fn flatten(steps: &Value, found: &mut Vec<(String, Option<String>)>) {
    for call in steps.as_array().map(Vec::as_slice).unwrap_or_default() {
        let Some(id) = call["path"].as_str().or_else(|| call["id"].as_str()) else { continue };
        found.push((id.to_owned(), call["step"].as_str().map(str::to_owned)));
        flatten(&call["steps"], found);
    }
}

fn source_kind(capability: &str) -> &str {
    match capability {
        "tracker" => ISSUE,
        "code" => "pull_request",
        other => other,
    }
}

fn mentions(input: &Value, subject: &str) -> bool {
    let sources = input["sources"].as_array().map(Vec::as_slice).unwrap_or_default();
    input["issue"] == subject || sources.iter().any(|source| source["subject"] == subject)
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
        shared.config.sync_config.clone(),
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
