use agent_engine::labels;
use agent_engine::policy_gate::{self, Asked};
use connectors::Labelled;
use domain::effect::{Call, IdempotencyKey};
use domain::label::{self, Levels, Mark, Sink};
use domain::permission::{Call as Judged, Effect, Layer, Reason, Surface, Verdict};
use domain::workflow::{WorkItem, WorkflowEvent};
use domain::{AskId, AskKind, Decision};
use serde_json::Value;

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
        let descriptor = hub.descriptor(&connection.provider).ok();
        let spec = descriptor.and_then(|descriptor| descriptor.action(&call.action));
        let declared = spec.and_then(|action| action.declaration().operation(&server, &call.args));
        let effects: Vec<Effect> = declared.map(Effect::Operation).into_iter().collect();
        let asked = Asked { surface: surface_of(self.item), stage: &self.key.step, owner: self.owner };
        let subject = policy_gate::subject(&call.action, &call.args);
        let verdict = match policy_gate::decide(asked, &Judged { tool: &tool, subject: &subject, effects: &effects }) {
            Verdict::Allow => self.flow(asked, &tool, spec),
            listed => listed,
        };
        match verdict {
            Verdict::Allow => Ok(()),
            Verdict::Deny(reason) => Err(Failure::permanent(policy_gate::refusal(&reason))),
            Verdict::Ask { reason: Reason::Flow(why), .. } if asked.surface == Surface::Routine => {
                let told = Reason::Flow(why).unattended(&tool);
                policy_gate::blocked(asked, &tool, &told, Layer::Declared);
                Err(Failure::permanent(policy_gate::refusal(&told)))
            }
            Verdict::Ask { .. } if asked.surface == Surface::Routine => {
                Err(Failure::permanent(policy_gate::unasked(asked, &tool)))
            }
            Verdict::Ask { reason: Reason::Flow(why), .. } => {
                self.approved(asked, &tool, &format!("{} {why}", call.action)).await
            }
            Verdict::Ask { .. } => self.approved(asked, &tool, &call.action).await,
        }
    }

    fn flow(&self, asked: Asked<'_>, tool: &str, spec: Option<&connectors::ActionSpec>) -> Verdict {
        let Some(spec) = spec else { return Verdict::Allow };
        let levels = self.levels();
        let mark = Mark::of(&levels, &label::reads(self.item));
        let sink = Sink { ceiling: spec.max_input_level.as_deref(), risky: spec.risky() };
        labels::flow(asked, tool, &levels, &mark, &sink)
    }

    fn levels(&self) -> Levels {
        let resources = self.shared.resources_in(self.shared.home(&self.item.input).as_deref());
        resources.find(labels::FILE).map(|found| labels::levels_of(&found.text)).unwrap_or_default()
    }

    pub fn read(&self, labelled: Labelled) -> Value {
        let read = labels::classified(&self.levels(), labelled.read, &labelled.value);
        let progress = WorkflowEvent::EffectProgress { key: self.key.clone(), data: read.progress() };
        if let Err(error) = self.shared.append_open(&self.item.id, &[progress], || {}) {
            tracing::warn!(%error, "what the call read could not be kept in the log");
        }
        labelled.value
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
