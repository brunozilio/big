use std::collections::HashMap;
use std::sync::Arc;

use domain::effect::EffectOutcome;
use domain::workflow::{ItemId, WorkItem, WorkflowEvent};

use crate::api::{Orchestrator, settle_ended};
use crate::asks::PendingAsk;
use crate::error::Result;
use crate::events::OrchestratorEvent;
use crate::shared::{Shared, timeout_timer};

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Killed {
    pub stopped: Vec<ItemId>,
    pub failed: Vec<(ItemId, String)>,
}

fn ending(shared: &Shared, state: &WorkItem, reason: &str, gone: &mut Vec<PendingAsk>) -> Vec<WorkflowEvent> {
    let mut events = Vec::new();
    for entry in state.effects.values().filter(|entry| entry.outcome.is_none()) {
        let key = &entry.request.key;
        gone.extend(shared.asks.forget(key));
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

fn kill_item(shared: &Arc<Shared>, id: &ItemId, reason: &str) -> Result<bool> {
    let mut gone = Vec::new();
    let Some(ended) = shared.end_with(id, |state| ending(shared, state, reason, &mut gone))? else { return Ok(false) };
    for pending in gone {
        shared.emit(OrchestratorEvent::AskClosed { item: id.clone(), ask: pending.ask.id });
    }
    shared.forget_waits(id, None);
    for child in ended.children.iter().filter(|child| !child.finished) {
        kill_item(shared, &child.id, reason)?;
    }
    tracing::info!(target: "audit", item = %id, reason, "item.killed");
    settle_ended(shared, &ended, reason, |shared, child, reason| kill_item(shared, child, reason).map(|_| ()))?;
    Ok(true)
}

impl Orchestrator {
    pub fn kill(&self, id: &ItemId) -> Result<()> {
        kill_item(&self.shared, id, crate::texts::killed(crate::texts::of_item(&self.shared, id))).map(|_| ())
    }

    pub fn kill_agent(&self, agent: &str) -> Result<Killed> {
        let active = self.shared.store.active_items()?;
        let running: Vec<&WorkItem> =
            active.iter().filter(|item| item.running_agents().iter().any(|named| named == agent)).collect();
        let parents: HashMap<&ItemId, Option<&ItemId>> =
            active.iter().map(|item| (&item.id, item.parent.as_ref())).collect();
        let killed_with_parent = |item: &WorkItem| {
            let mut above = item.parent.as_ref();
            while let Some(parent) = above {
                if running.iter().any(|found| &found.id == parent) {
                    return true;
                }
                above = parents.get(parent).copied().flatten();
            }
            false
        };
        let first: Vec<ItemId> =
            running.iter().filter(|item| !killed_with_parent(item)).map(|item| item.id.clone()).collect();
        let mut killed = Killed::default();
        for id in first {
            let reason = crate::texts::killed(crate::texts::of_item(&self.shared, &id));
            match kill_item(&self.shared, &id, reason) {
                Ok(true) => killed.stopped.push(id),
                Ok(false) => {}
                Err(error) => killed.failed.push((id, error.to_string())),
            }
        }
        Ok(killed)
    }
}
