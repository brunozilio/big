mod flows;
pub mod local;
pub mod outbox;
mod runs;
mod sent;

use std::sync::Arc;

use crate::api::{ClientCommand, Envelope, Reply, ServerEvent};
use agent_engine::EventSink;
use domain::{EngineEvent, PromptRequest, RunId};

use crate::handler::flows::Flows;
use crate::handler::outbox::{Outbox, Replies};
use crate::handler::runs::{Reach, RunTable};
use crate::services::LocalServices;

#[derive(Debug)]
pub struct CommandHandler {
    services: LocalServices,
    runs: Arc<RunTable>,
    outbox: Outbox,
    flows: Flows,
    client: u64,
}

impl Drop for CommandHandler {
    fn drop(&mut self) {
        agent_engine::tools::peers::forget(self.client);
        agent_engine::tools::builtin::shell::shells::kill_client(self.client);
        agent_engine::tools::builtin::shell::savings::forget(self.client);
        agent_engine::tools::inbox::forget_client(self.client);
        agent_engine::tools::watch::forget_client(self.client);
        agent_engine::team::lead_inbox::closing(self.client);
    }
}

impl CommandHandler {
    pub fn new(services: LocalServices, outbox: Outbox) -> Self {
        let runs = Arc::new(RunTable::default());
        let client = agent_engine::tools::clients::fresh();
        Self { services, runs, outbox, flows: Flows::default(), client }
    }

    pub fn handle(&self, envelope: Envelope<ClientCommand>) {
        let replies = self.outbox.answering(envelope.id);
        let command = envelope.payload;
        let name = command.name();
        tracing::debug!(command = name, request = %envelope.id, "handling command");
        let answered = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| self.answer(command, &replies)));
        match answered {
            Ok(Some(answer)) => replies.send(answer),
            Ok(None) => {}
            Err(_) => {
                tracing::error!(command = name, request = %envelope.id, "a command's handler panicked");
                replies.send(Reply::error(format!("internal error while handling {name}")));
            }
        }
    }

    fn answer(&self, command: ClientCommand, replies: &Replies) -> Option<Reply> {
        let providers = &self.services.providers;
        Some(match command {
            ClientCommand::SubmitPrompt { run, request } => {
                Reply::done_or(self.submit(run, &crate::handler::sent::as_sent(&request)))
            }
            ClientCommand::CancelRun { run } => Self::reached("cancel_run", run, self.runs.cancel(run)),
            ClientCommand::AnswerAsk { run, ask, decision } => {
                let reach = self.runs.control(run, |handle| handle.answer(&ask, &decision));
                Self::reached("answer_ask", run, reach)
            }
            ClientCommand::SteerRun { run, text } => {
                let reach = self.runs.control(run, |handle| handle.steer(text));
                Self::reached("steer_run", run, reach)
            }
            ClientCommand::ListModels => {
                providers.discover_models(self.services.engine.models(), replies);
                return None;
            }
            ClientCommand::ListProviders => providers.list(),
            ClientCommand::SignInProvider { provider, back, fields } => {
                self.flows.began(replies);
                providers.sign_in_returning(provider, back, fields, replies);
                return None;
            }
            ClientCommand::DisconnectProvider { provider } => providers.disconnect(provider),
            ClientCommand::RemoveProviderAccount { provider, account } => providers.remove_account(provider, &account),
            ClientCommand::ListSessions => Reply::Sessions(crate::sessions::listed(&self.services)),
            ClientCommand::SessionHistory { session, cwd } => {
                match crate::sessions::history(&self.services, &cwd, &session) {
                    Ok(records) => Reply::History(records),
                    Err(message) => Reply::error(message),
                }
            }
            ClientCommand::SessionHistorySince { session, cwd, known } => {
                match crate::sessions::history_since(&self.services, &cwd, &session, known) {
                    Ok((stamp, records)) => Reply::HistorySince { stamp, records },
                    Err(message) => Reply::error(message),
                }
            }
            ClientCommand::CancelRequest { request } => {
                if !self.flows.cancel(request) {
                    tracing::debug!(%request, "nothing left to cancel for this request");
                }
                Reply::Done
            }
            ClientCommand::BackgroundJobs => Reply::Background(self.background()),
            ClientCommand::StopBackground { id } => {
                let stopped = agent_engine::tools::clients::stop_background(self.client, &id);
                Reply::done_or(if stopped { Ok(()) } else { Err(format!("no background job {id} is running")) })
            }
        })
    }

    fn background(&self) -> Vec<crate::api::BackgroundJob> {
        agent_engine::tools::clients::background(self.client)
            .into_iter()
            .map(|(id, command, running)| crate::api::BackgroundJob { id, command, running })
            .collect()
    }

    pub fn cancel_all(&self) {
        self.runs.cancel_all();
    }

    fn submit(&self, run: RunId, request: &PromptRequest) -> Result<(), String> {
        if self.runs.is_active(run) {
            return Err(format!("run {run} is already in progress"));
        }
        if let Err(message) = self.start(run, request) {
            self.fail(run, message);
        }
        Ok(())
    }

    fn start(&self, run: RunId, request: &PromptRequest) -> Result<(), String> {
        let mut request = request.clone();
        request.host.credentials.mcp = Vec::new();
        let prepared = crate::prepare::prepare(&self.services, self.client, run, &request);
        let sink = Arc::new(RunSink {
            runs: Arc::clone(&self.runs),
            listing: crate::catalog::Listing::of(&self.services, &prepared.request),
            outbox: self.outbox.clone(),
            ending: prepared.ending,
        });
        match self.services.engine.start(run, &prepared.request, sink) {
            Ok(handle) => {
                self.runs.insert(run, handle);
                Ok(())
            }
            Err(error) => Err(error.to_string()),
        }
    }

    fn reached(command: &'static str, run: RunId, reach: Reach) -> Reply {
        if reach == Reach::Inactive {
            tracing::debug!(%run, command, "for a run that is not active");
        }
        Reply::Done
    }

    fn fail(&self, run: RunId, message: String) {
        self.outbox.send(ServerEvent::Run { run, event: EngineEvent::Failed(message) });
    }
}

#[cfg(test)]
mod tests;

struct RunSink {
    runs: Arc<RunTable>,
    listing: Option<crate::catalog::Listing>,
    outbox: Outbox,
    ending: crate::prepare::Ending,
}

impl EventSink for RunSink {
    fn emit(&self, run: RunId, event: EngineEvent) {
        if let Some(listing) = &self.listing {
            listing.observe(&event);
        }
        let finished = self.ending.observe(&event);
        if event.is_terminal() {
            self.runs.finish(run);
        }
        for finished in finished {
            self.outbox.send(ServerEvent::Unsolicited(finished));
        }
        self.outbox.send(ServerEvent::Run { run, event });
    }
}
