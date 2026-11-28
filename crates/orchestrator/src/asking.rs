use domain::effect::IdempotencyKey;
use domain::workflow::{ItemId, WorkflowEvent};
use domain::{AgentAsk, AskId, AskKind, Decision};
use serde_json::{Value, json};
use tokio::sync::broadcast::{Receiver, error::RecvError};

use crate::asks::PendingAsk;
use crate::events::OrchestratorEvent;
use crate::shared::{Shared, now_ms};

pub struct Asking<'a> {
    pub shared: &'a Shared,
    pub item: &'a ItemId,
    pub key: &'a IdempotencyKey,
    pub id: AskId,
}

impl Asking<'_> {
    pub async fn decided(&self, kind: AskKind) -> Option<Decision> {
        if let Some(decision) = self.answered() {
            return Some(decision);
        }
        let events = self.shared.events.subscribe();
        self.open(kind.clone());
        self.waited(events, &kind).await
    }

    async fn waited(&self, mut events: Receiver<OrchestratorEvent>, kind: &AskKind) -> Option<Decision> {
        loop {
            if let Some(decision) = self.shared.asks.take_early(self.key, kind) {
                return Some(decision);
            }
            if self.gone() {
                return None;
            }
            match events.recv().await {
                Ok(event) if self.concerns(&event) => {}
                Ok(_) | Err(RecvError::Lagged(_)) => continue,
                Err(RecvError::Closed) => return None,
            }
        }
    }

    fn concerns(&self, event: &OrchestratorEvent) -> bool {
        match event {
            OrchestratorEvent::ItemChanged { item, .. }
            | OrchestratorEvent::ItemEnded { item }
            | OrchestratorEvent::AskClosed { item, .. } => item == self.item,
            OrchestratorEvent::Session { .. } | OrchestratorEvent::Asked { .. } => false,
        }
    }

    fn gone(&self) -> bool {
        self.shared.item(self.item).map_or(true, |state| state.is_ended())
    }

    fn answered(&self) -> Option<Decision> {
        let state = self.shared.item(self.item).ok()?;
        let entry = state.effects.get(&self.key.to_string())?;
        serde_json::from_value(entry.progress["answered"][self.id.0.as_str()]["decision"].clone()).ok()
    }

    fn open(&self, kind: AskKind) {
        if self.shared.asks.of_key(self.key).iter().any(|pending| pending.ask.id == self.id) {
            return;
        }
        let pending = PendingAsk {
            item: self.item.clone(),
            step: self.key.step.clone(),
            session: None,
            ask: AgentAsk { id: self.id.clone(), kind },
            since: now_ms(),
        };
        let data = json!({ "asks": { self.id.0.clone(): serde_json::to_value(&pending).unwrap_or(Value::Null) } });
        let progress = WorkflowEvent::EffectProgress { key: self.key.clone(), data };
        let kept = self.shared.append_open(self.item, &[progress], || {
            self.shared.asks.restore(pending.clone(), self.key.clone());
        });
        match kept {
            Ok(true) => {}
            Ok(false) => return,
            Err(error) => {
                tracing::warn!(%error, "an ask could not be kept in the log");
                self.shared.asks.restore(pending.clone(), self.key.clone());
            }
        }
        let PendingAsk { item, step, session, ask, .. } = pending;
        self.shared.emit(OrchestratorEvent::Asked { item, step, session, ask });
    }
}
