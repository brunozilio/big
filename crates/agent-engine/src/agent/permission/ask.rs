use crossbeam_channel::RecvError;
use domain::operation::Disclosed;
use domain::permission::{Allowed, Effect, Judgement, Standing};
use domain::{AgentAsk, AskId, AskKind, Decision, EngineEvent};
use serde_json::Value;

use super::{Asking, Called};
use crate::agent::Agent;
use crate::tools::Outcome;

impl Agent<'_> {
    pub(super) fn ask_permission(&self, asking: &Asking<'_>, shown: &str, rememberable: bool) -> Outcome {
        let Asking { id, tool, called, .. } = *asking;
        let Called { name, input, effects, .. } = *called;
        self.notify(
            "permission_prompt",
            &format!("{brand_name} needs your permission to use {name}", brand_name = brand::NAME),
        );
        let disclosed = Disclosed::of(effects.iter().filter_map(|effect| match effect {
            Effect::Operation(operation) => Some(operation),
            _ => None,
        }));
        let kind = AskKind::Permission { tool: name.to_owned(), subject: shown.to_owned(), rememberable, disclosed };
        match self.asked_relaying(id, kind, Some(input)) {
            Ok(Decision::Allow | Decision::Answer(_) | Decision::Approve(_)) => self.went_ahead(id, tool, input),
            Ok(Decision::AllowAlways) => {
                if rememberable {
                    self.remember_allowed(asking);
                }
                self.went_ahead(id, tool, input)
            }
            Ok(Decision::Deny) => self.refused(
                name,
                format!("The user did not allow {name} here. Do not retry it; ask them or take another approach."),
            ),
            Ok(Decision::DenyWith(note)) => {
                self.refused(name, format!("The user did not allow {name} here and said what to do instead: {note}"))
            }
            Err(_) => Outcome::error(crate::tools::stopped::while_asking(name)),
        }
    }

    fn remember_allowed(&self, asking: &Asking<'_>) {
        let Judgement { standing, subject, .. } = asking.judgement;
        if *standing == Standing::Acts && domain::permission::allowed::global().is_some() {
            let learned = Allowed::learned(&asking.called.call());
            if !learned.is_empty() {
                domain::permission::allowed::change(|allowed| {
                    for grant in learned.into_iter().rev() {
                        if !allowed.has(&grant) {
                            allowed.add_first(grant);
                        }
                    }
                });
                return;
            }
        }
        self.control.remember(|remembered| remembered.remember(asking.called.name, *standing, subject));
    }

    pub(in crate::agent) fn asked(&self, call: &str, kind: AskKind) -> Result<Decision, RecvError> {
        self.asked_relaying(call, kind, None)
    }

    fn asked_relaying(&self, call: &str, kind: AskKind, input: Option<&Value>) -> Result<Decision, RecvError> {
        let ask = AskId(format!("ask-{}-{call}", uuid::Uuid::new_v4()));
        let mut answer = self.control.open(ask.clone());
        let relayed = match (&kind, input) {
            (AskKind::Permission { tool, subject, .. }, Some(input)) => Some((tool.clone(), subject.clone(), input)),
            _ => None,
        };
        (self.emit)(EngineEvent::Asked(AgentAsk { id: ask.clone(), kind }));
        let _relay = relayed.and_then(|(tool, subject, input)| {
            let client = self.workspace.credentials.client;
            crate::mcp::channel_relay::ask(client, call, &tool, &subject, input, self.control.answerer(&ask))
        });
        let answer = answer.recv();
        if let Ok(decision) = &answer {
            (self.emit)(EngineEvent::Answered { ask, decision: decision.clone() });
        }
        answer
    }

    pub(in crate::agent) fn notify(&self, kind: &str, message: &str) {
        let input = serde_json::json!({ "message": message, "notification_type": kind });
        if let Err(error) = self.workspace.hooks.run(crate::hooks::HookEvent::Notification, kind, input, self.workspace)
        {
            tracing::warn!(%error, "a Notification hook failed");
        }
    }
}
