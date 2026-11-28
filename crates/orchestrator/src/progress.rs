use std::sync::{Arc, PoisonError};

use connectors::ExternalEvent;
use domain::connection::{Capability, ConnectionId};
use domain::workflow::{ItemId, StepState, WorkItem, WorkflowEvent};
use serde_json::{Value, json};

use crate::api::Orchestrator;
use crate::error::Result;
use crate::events::Wake;
use crate::links::{CAPABILITY, application_of, base_of, claimed_by_peers, integrated};
use crate::shared::Shared;

pub const STEP_FINISHED: &str = "item.step.finished";
pub const ACCEPTING: &str = "accepting";

#[derive(Debug, Clone, PartialEq)]
pub struct Progress {
    pub item: ItemId,
    pub step: String,
    pub state: StepState,
    pub version: u64,
}

pub fn finished(item: &ItemId, version: u64, events: &[WorkflowEvent]) -> Vec<Progress> {
    events
        .iter()
        .filter_map(|event| match event {
            WorkflowEvent::StepFinished { step, state, .. } => {
                Some(Progress { item: item.clone(), step: step.clone(), state: *state, version })
            }
            _ => None,
        })
        .collect()
}

pub fn event_of(progress: &Progress, item: &WorkItem) -> ExternalEvent {
    let state = serde_json::to_value(progress.state).unwrap_or(Value::Null);
    ExternalEvent {
        id: format!("{}/{}/{}", progress.item, progress.step, progress.version),
        connection: ConnectionId::from(CAPABILITY),
        capability: Capability::from(CAPABILITY),
        kind: STEP_FINISHED.to_owned(),
        subject: progress.item.to_string(),
        data: json!({
            "item": progress.item,
            "workflow": item.workflow,
            "step": progress.step,
            "state": state,
            "application": application_of(item),
            "base": base_of(item),
            "integrated": integrated(item),
            "title": crate::views::item_json(item)["title"],
        }),
        at: jiff::Timestamp::now(),
        scope: String::new(),
        aliases: Vec::new(),
    }
}

pub fn watch(shared: &Arc<Shared>) {
    let shared = Arc::clone(shared);
    let owner = Arc::clone(&shared);
    owner.spawn(async move {
        loop {
            let notified = shared.progressing.notified();
            let pending: Vec<Progress> =
                std::mem::take(&mut *shared.progress.lock().unwrap_or_else(PoisonError::into_inner));
            if pending.is_empty() {
                notified.await;
                continue;
            }
            let worker = Arc::clone(&shared);
            let done = tokio::task::spawn_blocking(move || {
                let orchestrator = Orchestrator::of(worker);
                for progress in pending {
                    if let Err(error) = orchestrator.announce(&progress) {
                        tracing::warn!(item = %progress.item, %error, "the progress of an item could not be announced");
                    }
                }
            });
            let _ = done.await;
        }
    });
}

fn trigger_of(shape: &Value, kind: &str) -> Value {
    let triggers = shape["triggers"].as_array().cloned().unwrap_or_default();
    triggers.into_iter().find(|on| on["kind"] == kind).unwrap_or(Value::Null)
}

fn fresh_input(trigger: &Value, origin: &WorkItem, event: &Value) -> Value {
    let mut input = json!({ "title": trigger["title"].as_str().unwrap_or_default(), "event": event });
    for field in ["application", "language", "model", "effort"] {
        if !origin.input[field].is_null() {
            input[field] = origin.input[field].clone();
        }
    }
    input
}

impl Orchestrator {
    pub(crate) fn announce(&self, progress: &Progress) -> Result<usize> {
        let origin = self.shared.item(&progress.item)?;
        let event = event_of(progress, &origin);
        self.deliver_event(&event)?;
        self.offer(&event, &origin)
    }

    pub(crate) fn offer(&self, event: &ExternalEvent, origin: &WorkItem) -> Result<usize> {
        let value = serde_json::to_value(event).unwrap_or(Value::Null);
        let all = self.shared.store.items()?;
        let mut made = 0;
        for workflow in self.triggered(&event.names(), &value) {
            if workflow == origin.workflow || claimed_by_peers(&all, &workflow).contains(&origin.id) {
                continue;
            }
            let Some(shape) = self.shape(&workflow) else { continue };
            let trigger = trigger_of(&shape, &event.kind);
            let accepting = trigger["route"] == ACCEPTING;
            let open = all.iter().find(|peer| {
                peer.workflow == workflow
                    && !peer.is_ended()
                    && peer.accepting
                    && application_of(peer) == application_of(origin)
            });
            if let Some(open) = open.filter(|_| accepting) {
                self.shared.wake(&open.id, Wake::Poke);
                continue;
            }
            let id = self.create_with(&workflow, fresh_input(&trigger, origin, &value), None)?;
            if accepting {
                self.shared.append(&id, &[WorkflowEvent::Accepting { on: true }])?;
            }
            made += 1;
        }
        Ok(made)
    }
}
