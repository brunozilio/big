use std::time::Instant;

use agent_engine::conduct::told;
use agent_engine::labels;
use agent_engine::policy_gate::{self, Asked};
use connectors::{ActionSpec, Labelled};
use domain::audit::{AuthorizedBy, OperationTag, code};
use domain::effect::{Call, IdempotencyKey};
use domain::label::{self, Held, Levels, Mark, Sink};
use domain::permission::{AskOutcome, Call as Judged, Effect, Layer, Reason, Surface, Verdict};
use domain::workflow::{WorkItem, WorkflowEvent};
use domain::{AskId, AskKind, Decision};
use serde_json::Value;
use tracing::Instrument;

use crate::agent::Failure;
use crate::asking::Asking;
use crate::shared::Shared;

pub fn surface_of(item: &WorkItem) -> Surface {
    if item.input["schedule"].is_object() { Surface::Routine } else { Surface::Workflow }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Authorized {
    pub tag: OperationTag,
    pub by: AuthorizedBy,
    pub waited_ms: Option<u64>,
}

impl Authorized {
    fn standing(tag: OperationTag) -> Self {
        Self { tag, by: AuthorizedBy::Standing, waited_ms: None }
    }

    pub fn code(&self) -> &'static str {
        match self.by {
            AuthorizedBy::Standing => code::STANDING,
            AuthorizedBy::Approval(_) => code::GRANTED,
        }
    }
}

fn tag_of(call: &Call, spec: Option<&ActionSpec>) -> OperationTag {
    call.operation.clone().or_else(|| spec.map(ActionSpec::tag)).unwrap_or_else(|| OperationTag::plain(&call.action))
}

pub struct Gated<'a> {
    pub shared: &'a Shared,
    pub item: &'a WorkItem,
    pub key: &'a IdempotencyKey,
    pub owner: &'a str,
}

impl Gated<'_> {
    pub async fn check(&self, call: &Call) -> Result<Authorized, Failure> {
        let Ok((connection, _)) = crate::effects::connection_of(self.shared, self.owner, &self.item.input, call) else {
            return Ok(Authorized::standing(tag_of(call, None)));
        };
        let server = domain::mcp::connectors_server();
        let tool = domain::mcp::served_name(&server, &domain::mcp::connector_tool(&connection.label, &call.action));
        let hub = &self.shared.config.connectors;
        let descriptor = hub.descriptor(&connection.provider).ok();
        let spec = descriptor.and_then(|descriptor| descriptor.action(&call.action));
        let tag = tag_of(call, spec);
        let span = tracing::info_span!(
            target: "audit",
            "operation",
            operation.id = %tag.id,
            operation.version = tag.version.as_deref(),
            operation.name = tag.friendly_name.as_deref()
        );
        let named = Named { tool: &tool, operation: &tag.id };
        let (by, waited_ms) = self.judged(call, &server, named, spec).instrument(span).await?;
        Ok(Authorized { tag, by, waited_ms })
    }

    async fn judged(
        &self,
        call: &Call,
        server: &str,
        named: Named<'_>,
        spec: Option<&ActionSpec>,
    ) -> Result<(AuthorizedBy, Option<u64>), Failure> {
        let tool = named.tool;
        let declared = spec.and_then(|action| action.declaration().operation(server, &call.args));
        let effects: Vec<Effect> = declared.map(Effect::Operation).into_iter().collect();
        let asked = Asked {
            surface: surface_of(self.item),
            stage: &self.key.step,
            owner: self.owner,
            operation: Some(named.operation),
        };
        let subject = policy_gate::subject(&call.action, &call.args);
        let verdict = match policy_gate::decide(asked, &Judged { tool, subject: &subject, effects: &effects }) {
            Verdict::Allow => self.flow(asked, tool, spec),
            listed => listed,
        };
        match verdict {
            Verdict::Allow => Ok((AuthorizedBy::Standing, None)),
            Verdict::Deny(reason) => Err(Failure::permanent(policy_gate::refusal(&reason))),
            Verdict::Ask { reason: reason @ Reason::Flow(_), .. } if asked.surface == Surface::Routine => {
                Err(Failure::permanent(policy_gate::unattended(asked, tool, &reason, code::FLOW_UNATTENDED)))
            }
            Verdict::Ask { reason: reason @ Reason::Two(_), .. } if asked.surface == Surface::Routine => {
                Err(Failure::permanent(policy_gate::unattended(asked, tool, &reason, code::TWO_UNATTENDED)))
            }
            Verdict::Ask { .. } if asked.surface == Surface::Routine => {
                Err(Failure::permanent(policy_gate::unasked(asked, tool)))
            }
            Verdict::Ask { reason: Reason::Flow(why) | Reason::Two(why), .. } => {
                self.approved(asked, named, &format!("{} {why}", call.action)).await
            }
            Verdict::Ask { .. } => self.approved(asked, named, &call.action).await,
        }
    }

    fn flow(&self, asked: Asked<'_>, tool: &str, spec: Option<&ActionSpec>) -> Verdict {
        let Some(spec) = spec else { return Verdict::Allow };
        let levels = self.levels();
        let mark = Mark::of(&levels, &label::reads(self.item));
        let sink = Sink { ceiling: spec.max_input_level.as_deref(), risky: spec.risky() };
        match labels::flow(asked, tool, &levels, &mark, &sink) {
            Verdict::Allow => {
                let held = Held::of(&levels, &label::reads(self.item));
                labels::paired(asked, &levels, &held, &spec.next(tool))
            }
            crossed => crossed,
        }
    }

    fn levels(&self) -> Levels {
        let resources = self.shared.resources_in(self.shared.home(&self.item.input).as_deref());
        resources.find(labels::FILE).map(|found| labels::levels_of(&found.text)).unwrap_or_default()
    }

    pub fn read(&self, labelled: Labelled) -> Result<Value, Failure> {
        let read = labels::classified(&self.levels(), labelled.read, &labelled.value);
        let progress = WorkflowEvent::EffectProgress { key: self.key.clone(), data: read.progress() };
        self.shared.append_open(&self.item.id, &[progress], || {}).map_err(|error| {
            let told = format!("what the call read could not be kept in the log: {error}");
            Failure::Class(domain::effect::ErrorClass::Transient, told)
        })?;
        Ok(labelled.value)
    }

    async fn approved(
        &self,
        asked: Asked<'_>,
        named: Named<'_>,
        subject: &str,
    ) -> Result<(AuthorizedBy, Option<u64>), Failure> {
        let tool = named.tool;
        let id = AskId(format!("policy-{}", self.key));
        let asking = Asking { shared: self.shared, item: &self.item.id, key: self.key, id: id.clone() };
        let started = Instant::now();
        let Some(decision) = asking.decided(AskKind::permission(tool, subject, false)).await else {
            return Err(Failure::permanent(ended(asked, named, AskOutcome::Expired, code::ABANDONED)));
        };
        settled(asked, named, &decision)?;
        let measured = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
        Ok((AuthorizedBy::Approval(id.0), Some(asking.waited_ms().unwrap_or(measured))))
    }
}

#[derive(Debug, Clone, Copy)]
struct Named<'a> {
    tool: &'a str,
    operation: &'a str,
}

fn settled(asked: Asked<'_>, named: Named<'_>, decision: &Decision) -> Result<(), Failure> {
    match decision {
        Decision::Allow | Decision::AllowAlways | Decision::Approve(_) => Ok(()),
        Decision::Deny | Decision::DenyWith(_) | Decision::Answer(_) => {
            Err(Failure::permanent(ended(asked, named, AskOutcome::Declined, code::REFUSED)))
        }
    }
}

fn ended(asked: Asked<'_>, named: Named<'_>, outcome: AskOutcome, decision: &str) -> String {
    let reason = told(&asked.told(outcome, named.tool));
    policy_gate::blocked_as(asked, named.tool, &reason, Layer::List, decision);
    outcome.refusal(&reason)
}
