use domain::EngineEvent;
use domain::data_policy::{Rerouted, Skipped};
use domain::fallback::{Failure, Fallback, FallbackChoice};
use serde_json::{Value, json};

use super::{Exchange, Next, TurnState, cutoffs};
use crate::agent::run::ThisRun;
use crate::agent::{Agent, Ending};
use crate::hooks::HookOutcome;
use crate::model::{BlockingTurn, Delta, ModelError, Reply, ToolSpec, TurnRequest};

const RECOVERIES_AT_MOST: u8 = 2;

impl Agent<'_> {
    pub(super) fn ask_model(&self, turn: &mut TurnState<'_>, run: &ThisRun) -> Result<Next, Ending> {
        if self.control.is_stopped() {
            return Err(Ending::Cancelled);
        }
        let budget = self.upkeep(turn)?;
        let reminder = self.runtime_reminder(&budget, turn.reminders.quiet_tasks(), run);
        turn.reminders.deliver(reminder, turn.history);
        run.transition(turn.history);
        let mut request = TurnRequest { messages: turn.history.clone(), ..self.turn_template(turn.fell_back.as_ref()) };
        self.workspace.policy.redact_prompt(&mut request.messages);
        self.intercept_request(&mut request, &turn.rejections)?;
        if let Some(why) = self.workspace.models.unmet(&request.model) {
            let refused = crate::data_gate::refused(&[Skipped { model: request.model.clone(), why }]);
            return self.recovered(turn, &request, refused, "");
        }
        let mut shown = String::new();
        match self.answered(&request, &mut shown) {
            Ok(reply) => {
                turn.fallback.succeeded();
                turn.usage.add(reply.usage);
                Ok(Next::Replied(Box::new(Exchange { request, reply })))
            }
            Err(error) => self.recovered(turn, &request, error, &shown),
        }
    }

    pub(super) fn streamed(&self, request: &TurnRequest, shown: &mut String) -> Result<Reply, ModelError> {
        let holds_text = self.holds_text();
        let mut on = |delta: Delta| match delta {
            Delta::Text(_) if holds_text => {}
            Delta::Text(text) => (self.emit)(EngineEvent::TextDelta(cutoffs::streamed(shown, text))),
            Delta::Thinking(text) => (self.emit)(EngineEvent::ThinkingDelta(text)),
            Delta::ToolInput { name, piece } => (self.emit)(EngineEvent::ToolInputDelta { name, piece }),
            Delta::Searching { id, query } => {
                (self.emit)(EngineEvent::ToolStarted(domain::ToolCall::new(id, crate::tools::WEB_SEARCH, query)));
            }
            Delta::Searched { id, found } => {
                (self.emit)(EngineEvent::ToolResult { id: id.clone(), output: found, failed: false });
                (self.emit)(EngineEvent::ToolFinished { id });
            }
            Delta::HandedOver { from, to } => (self.emit)(EngineEvent::AccountSwitched { from, to }),
        };
        self.api.turn_blocking(request, &mut on, self.control.token())
    }

    fn recovered(
        &self,
        turn: &mut TurnState<'_>,
        request: &TurnRequest,
        error: ModelError,
        shown: &str,
    ) -> Result<Next, Ending> {
        match error {
            ModelError::Cancelled => Err(cutoffs::interrupted(turn.history, shown)),
            ModelError::TooLong(_) if turn.recoveries < RECOVERIES_AT_MOST => {
                turn.recoveries += 1;
                self.compact_to_fit(turn)?;
                Ok(Next::AskModel)
            }
            error => match self.next_model(&mut turn.fallback, request, &error) {
                Ok(Some(model)) => {
                    (self.emit)(EngineEvent::FellBack(model.clone()));
                    turn.fell_back = Some(model);
                    Ok(Next::AskModel)
                }
                Ok(None) => Err(Ending::Failed(error)),
                Err(refused) => Err(Ending::Failed(refused)),
            },
        }
    }

    fn next_model(
        &self,
        fallback: &mut Fallback,
        request: &TurnRequest,
        error: &ModelError,
    ) -> Result<Option<String>, ModelError> {
        let Some(failure) = error.failure() else { return Ok(None) };
        fallback.failed(&request.model);
        let extra = json!({
            "error": error.to_string(),
            "errorKind": failure.label(),
            "attempt": fallback.tried().len(),
            "tried": fallback.tried(),
            "fallbacks": fallback.chain(),
        });
        let verdict = match self.model_hook(crate::hooks::HookEvent::ModelError, request, extra) {
            Some(HookOutcome { blocked: Some(_), .. }) => FallbackChoice::Stop,
            Some(HookOutcome { model: Some(model), .. }) => FallbackChoice::Forward(model),
            _ => FallbackChoice::Pass,
        };
        let (to, skipped) = self.allowed(fallback, &request.model, failure, verdict);
        if to.is_some() || !skipped.is_empty() {
            let failure = failure.label().to_owned();
            let decision = Rerouted { from: request.model.clone(), to: to.clone(), failure, skipped: skipped.clone() };
            (self.emit)(EngineEvent::Rerouted(decision));
        }
        match to {
            None if !skipped.is_empty() => Err(crate::data_gate::refused(&skipped)),
            to => Ok(to),
        }
    }

    fn allowed(
        &self,
        fallback: &mut Fallback,
        from: &str,
        failure: Failure,
        verdict: FallbackChoice,
    ) -> (Option<String>, Vec<Skipped>) {
        let mut skipped = Vec::new();
        let mut next = fallback.next(from, failure, verdict);
        while let Some(model) = next {
            let Some(why) = self.workspace.models.unmet(&model) else { return (Some(model), skipped) };
            next = fallback.next(&model, failure, FallbackChoice::Pass);
            skipped.push(Skipped { model, why });
        }
        (None, skipped)
    }

    pub(super) fn model_hook(
        &self,
        event: crate::hooks::HookEvent,
        request: &TurnRequest,
        extra: Value,
    ) -> Option<HookOutcome> {
        if !self.workspace.hooks.listens_to(event) {
            return None;
        }
        let kind = if self.depth > 0 { "sub-agent" } else { "main" };
        let mut input = json!({
            "model": request.model,
            "kind": kind,
            "mode": domain::command::mode_label(self.control.mode()),
            "messages": request.messages.len(),
        });
        if let (Some(input), Value::Object(extra)) = (input.as_object_mut(), extra) {
            input.extend(extra);
        }
        match self.workspace.hooks.run(event, kind, input, self.workspace) {
            Ok(decision) => Some(decision),
            Err(error) => {
                tracing::warn!(%error, event = event.as_str(), "a model hook failed; the model stays as it was");
                None
            }
        }
    }

    pub(super) fn turn_template(&self, fell_back: Option<&String>) -> TurnRequest {
        TurnRequest {
            messages: Vec::new(),
            system: self.request.system.clone(),
            tools: self.offered(),
            model: fell_back.cloned().unwrap_or_else(|| self.request.model.clone()),
            ..self.request.clone()
        }
    }

    pub(super) fn offered(&self) -> Vec<ToolSpec> {
        if self.workspace.policy.code_mode == domain::code_mode::Mode::Only {
            let more_to_find =
                self.workspace.services.deferred.as_ref().is_some_and(|deferred| !deferred.waiting().is_empty());
            return crate::tools::repl::only(self.request.tools.clone(), more_to_find);
        }
        let mut tools = self.request.tools.clone();
        if let Some(deferred) = &self.workspace.services.deferred {
            tools.extend(deferred.loaded());
        }
        tools
    }
}
