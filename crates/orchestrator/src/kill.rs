use std::sync::Arc;

use domain::effect::{Effect, EffectOutcome};
use domain::workflow::{ItemId, WorkItem, WorkflowEvent};

use crate::api::{Orchestrator, settle_ended};
use crate::error::Result;
use crate::events::OrchestratorEvent;
use crate::shared::{Shared, timeout_timer};

fn ending(state: &WorkItem, reason: &str) -> Vec<WorkflowEvent> {
    let mut events = Vec::new();
    for entry in state.effects.values().filter(|entry| entry.outcome.is_none()) {
        let key = &entry.request.key;
        events.push(WorkflowEvent::EffectResolved { key: key.clone(), outcome: EffectOutcome::Cancelled });
        for timer in [key.to_string(), timeout_timer(key)] {
            if state.timers.contains_key(&timer) {
                events.push(WorkflowEvent::TimerFired { id: timer });
            }
        }
    }
    events.push(WorkflowEvent::Cancelled { reason: reason.to_owned() });
    events
}

fn kill_item(shared: &Arc<Shared>, id: &ItemId, reason: &str) -> Result<()> {
    let open: Vec<_> = shared.item(id)?.effects.values().map(|entry| entry.request.key.clone()).collect();
    let Some(ended) = shared.end_with(id, |state| ending(state, reason))? else { return Ok(()) };
    for key in &open {
        for gone in shared.asks.forget(key) {
            shared.emit(OrchestratorEvent::AskClosed { item: id.clone(), ask: gone.ask.id });
        }
    }
    shared.forget_waits(id, None);
    for child in ended.children.iter().filter(|child| !child.finished) {
        kill_item(shared, &child.id, reason)?;
    }
    tracing::info!(target: "audit", item = %id, reason, "item.killed");
    settle_ended(shared, &ended, reason, kill_item)
}

fn runs(item: &WorkItem, agent: &str) -> bool {
    item.effects.values().any(|entry| {
        entry.outcome.is_none()
            && matches!(&entry.request.effect, Effect::AgentRun { agent: named, .. } if named == agent)
    })
}

impl Orchestrator {
    pub fn kill(&self, id: &ItemId) -> Result<()> {
        kill_item(&self.shared, id, crate::texts::killed(crate::texts::of_item(&self.shared, id)))
    }

    pub fn kill_agent(&self, agent: &str) -> Result<Vec<ItemId>> {
        let running: Vec<ItemId> = self
            .shared
            .store
            .active_items()?
            .into_iter()
            .filter(|item| runs(item, agent))
            .map(|item| item.id)
            .collect();
        for id in &running {
            self.kill(id)?;
        }
        Ok(running)
    }
}
