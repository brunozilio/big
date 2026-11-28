use std::time::Duration;

use agent_engine::policy_gate::{self, Asked};
use domain::effect::{Call, IdempotencyKey};
use domain::permission::{Call as Judged, Effect, Surface, Verdict};
use domain::workflow::WorkItem;
use domain::{AgentAsk, AskId, AskKind, Decision};
use serde_json::{Value, json};

use crate::agent::Failure;
use crate::asks::PendingAsk;
use crate::events::OrchestratorEvent;
use crate::shared::{Shared, now_ms};

const WAIT: Duration = Duration::from_millis(200);

pub fn surface_of(item: &WorkItem) -> Surface {
    if item.input["schedule"].is_object() { Surface::Routine } else { Surface::Workflow }
}

pub struct Gated<'a> {
    pub shared: &'a Shared,
    pub item: &'a WorkItem,
    pub key: &'a IdempotencyKey,
    pub owner: &'a str,
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
        match policy_gate::decide(asked, &Judged { tool: &tool, subject: &call.action, effects: &effects }) {
            Verdict::Allow => Ok(()),
            Verdict::Deny(reason) => Err(Failure::permanent(policy_gate::refusal(&reason))),
            Verdict::Ask { .. } => self.approved(asked, &tool, &call.action).await,
        }
    }

    fn id(&self) -> AskId {
        AskId(format!("policy-{}", self.key))
    }

    async fn approved(&self, asked: Asked<'_>, tool: &str, subject: &str) -> Result<(), Failure> {
        let kind = AskKind::permission(tool, subject, false);
        if let Some(decision) = self.answered() {
            return settled(asked, tool, &decision);
        }
        self.open(kind.clone());
        loop {
            if let Some(decision) = self.shared.asks.take_early(self.key, &kind) {
                return settled(asked, tool, &decision);
            }
            tokio::time::sleep(WAIT).await;
        }
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
            policy_gate::blocked(asked, tool, &reason, domain::permission::Layer::List);
            Err(Failure::permanent(policy_gate::refusal(&reason)))
        }
    }
}
