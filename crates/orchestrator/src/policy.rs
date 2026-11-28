use agent_engine::policy_gate::{self, Asked};
use domain::effect::{Call, IdempotencyKey};
use domain::permission::{Call as Judged, Effect, Layer, Surface, Verdict};
use domain::workflow::WorkItem;
use domain::{AgentAsk, AskId, AskKind, Decision};
use serde_json::{Value, json};
use tokio::sync::broadcast::error::RecvError;

use crate::agent::Failure;
use crate::asks::PendingAsk;
use crate::events::OrchestratorEvent;
use crate::shared::{Shared, now_ms};

pub fn surface_of(item: &WorkItem) -> Surface {
    if item.input["schedule"].is_object() { Surface::Routine } else { Surface::Workflow }
}

pub struct Gated<'a> {
    pub shared: &'a Shared,
    pub item: &'a WorkItem,
    pub key: &'a IdempotencyKey,
    pub owner: &'a str,
}

enum Waited {
    Decided(Decision),
    Gone,
}

impl Gated<'_> {
    pub async fn check(&self, call: &Call) -> Result<(), Failure> {
        let Ok((connection, _)) = crate::effects::connection_of(self.shared, self.owner, call) else { return Ok(()) };
        let server = domain::mcp::connectors_server();
        let tool = domain::mcp::served_name(&server, &domain::mcp::connector_tool(&connection.label, &call.action));
        let hub = &self.shared.config.connectors;
        let declared = hub.descriptor(&connection.provider).ok().and_then(|descriptor| {
            let action = descriptor.actions.iter().find(|action| action.name == call.action)?;
            action.declaration().operation(&server, &call.args)
        });
        let effects: Vec<Effect> = declared.map(Effect::Operation).into_iter().collect();
        let asked = Asked { surface: surface_of(self.item), stage: &self.key.step, owner: self.owner };
        let subject = policy_gate::subject(&call.action, &call.args);
        match policy_gate::decide(asked, &Judged { tool: &tool, subject: &subject, effects: &effects }) {
            Verdict::Allow => Ok(()),
            Verdict::Deny(reason) => Err(Failure::permanent(policy_gate::refusal(&reason))),
            Verdict::Ask { .. } if asked.surface == Surface::Routine => {
                Err(Failure::permanent(policy_gate::unasked(asked, &tool)))
            }
            Verdict::Ask { .. } => self.approved(asked, &tool, &call.action).await,
        }
    }

    fn id(&self) -> AskId {
        AskId(format!("policy-{}", self.key))
    }

    async fn approved(&self, asked: Asked<'_>, tool: &str, subject: &str) -> Result<(), Failure> {
        let kind = AskKind::permission(tool, subject, false);
        let waited = match self.answered() {
            Some(decision) => Waited::Decided(decision),
            None => {
                let events = self.shared.events.subscribe();
                self.open(kind.clone());
                self.waited(events, &kind).await
            }
        };
        match waited {
            Waited::Decided(decision) => settled(asked, tool, &decision),
            Waited::Gone => {
                let reason = format!("{tool} was waiting for a person, and the item stopped first.");
                policy_gate::blocked(asked, tool, &reason, Layer::List);
                Err(Failure::permanent(policy_gate::refusal(&reason)))
            }
        }
    }

    async fn waited(&self, mut events: tokio::sync::broadcast::Receiver<OrchestratorEvent>, kind: &AskKind) -> Waited {
        loop {
            if let Some(decision) = self.shared.asks.take_early(self.key, kind) {
                return Waited::Decided(decision);
            }
            if self.gone() {
                return Waited::Gone;
            }
            match events.recv().await {
                Ok(event) if self.concerns(&event) => {}
                Ok(_) | Err(RecvError::Lagged(_)) => continue,
                Err(RecvError::Closed) => return Waited::Gone,
            }
        }
    }

    fn concerns(&self, event: &OrchestratorEvent) -> bool {
        match event {
            OrchestratorEvent::ItemChanged { item, .. }
            | OrchestratorEvent::ItemEnded { item }
            | OrchestratorEvent::AskClosed { item, .. } => *item == self.item.id,
            OrchestratorEvent::Session { .. } | OrchestratorEvent::Asked { .. } => false,
        }
    }

    fn gone(&self) -> bool {
        self.shared.item(&self.item.id).map_or(true, |state| state.is_ended())
    }

    fn answered(&self) -> Option<Decision> {
        let state = self.shared.item(&self.item.id).ok()?;
        let entry = state.effects.get(&self.key.to_string())?;
        serde_json::from_value(entry.progress["answered"][self.id().0.as_str()]["decision"].clone()).ok()
    }

    fn open(&self, kind: AskKind) {
        let id = self.id();
        if self.shared.asks.of_key(self.key).iter().any(|pending| pending.ask.id == id) {
            return;
        }
        let pending = PendingAsk {
            item: self.item.id.clone(),
            step: self.key.step.clone(),
            session: None,
            ask: AgentAsk { id: id.clone(), kind },
            since: now_ms(),
        };
        self.shared.asks.restore(pending.clone(), self.key.clone());
        let data = json!({ "asks": { id.0.clone(): serde_json::to_value(&pending).unwrap_or(Value::Null) } });
        let progress = domain::workflow::WorkflowEvent::EffectProgress { key: self.key.clone(), data };
        if let Err(error) = self.shared.append(&self.item.id, &[progress]) {
            tracing::warn!(%error, "a policy ask could not be kept in the log");
        }
        let PendingAsk { item, step, session, ask, .. } = pending;
        self.shared.emit(OrchestratorEvent::Asked { item, step, session, ask });
    }
}

fn settled(asked: Asked<'_>, tool: &str, decision: &Decision) -> Result<(), Failure> {
    match decision {
        Decision::Allow | Decision::AllowAlways | Decision::Approve(_) => Ok(()),
        Decision::Deny | Decision::DenyWith(_) | Decision::Answer(_) => {
            let reason = format!("{tool} was refused by the person asked.");
            policy_gate::blocked(asked, tool, &reason, Layer::List);
            Err(Failure::permanent(policy_gate::refusal(&reason)))
        }
    }
}
