use agent_engine::policy_gate::{self, Asked};
use domain::effect::{Call, IdempotencyKey};
use domain::permission::{Call as Judged, Effect, Layer, Surface, Verdict};
use domain::workflow::WorkItem;
use domain::{AskId, AskKind, Decision};

use crate::agent::Failure;
use crate::asking::Asking;
use crate::shared::Shared;

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

    async fn approved(&self, asked: Asked<'_>, tool: &str, subject: &str) -> Result<(), Failure> {
        let id = AskId(format!("policy-{}", self.key));
        let asking = Asking { shared: self.shared, item: &self.item.id, key: self.key, id };
        match asking.decided(AskKind::permission(tool, subject, false)).await {
            Some(decision) => settled(asked, tool, &decision),
            None => {
                let reason = format!("{tool} was waiting for a person, and the item stopped first.");
                policy_gate::blocked(asked, tool, &reason, Layer::List);
                Err(Failure::permanent(policy_gate::refusal(&reason)))
            }
        }
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
