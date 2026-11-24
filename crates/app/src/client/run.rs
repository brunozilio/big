use std::sync::{Arc, Mutex};

use platform::sync::Locked;

use crate::api::{ClientCommand, Reply};
use crossbeam_channel::Receiver;
use domain::session_run::{Carried, RunWatch};
use domain::{AskId, Decision, EngineEvent, RunId};

use crate::Error;
use crate::client::dispatch::{Dispatcher, Pending};

const BACKEND_GONE: &str = "the backend went away before the run ended";

#[derive(Debug)]
pub struct Run {
    id: RunId,
    events: Receiver<EngineEvent>,
    accepted: Option<Pending>,
    dispatcher: Arc<Dispatcher>,
    carried: Arc<Mutex<Carried>>,
    watch: RunWatch,
    done: bool,
}

impl Run {
    pub(crate) fn start(
        dispatcher: &Arc<Dispatcher>,
        command: impl FnOnce(RunId) -> ClientCommand,
    ) -> Result<Self, Error> {
        let run = RunId::new();
        let events = dispatcher.follow(run);
        let accepted = match dispatcher.ask(command(run)) {
            Ok(accepted) => accepted,
            Err(error) => {
                dispatcher.forget(run);
                return Err(error);
            }
        };
        Ok(Run {
            id: run,
            events,
            accepted: Some(accepted),
            dispatcher: Arc::clone(dispatcher),
            carried: Arc::default(),
            watch: RunWatch::default(),
            done: false,
        })
    }

    pub(crate) fn of_session(mut self, carried: Arc<Mutex<Carried>>, watch: RunWatch) -> Self {
        (self.carried, self.watch) = (carried, watch);
        self
    }

    pub fn next_event(&mut self) -> Option<EngineEvent> {
        if self.done {
            return None;
        }
        let event = self.awaited();
        self.observe(&event);
        Some(event)
    }

    fn awaited(&mut self) -> EngineEvent {
        loop {
            let Some(accepted) = self.accepted.as_ref().map(|pending| pending.replies().clone()) else {
                return self.events.recv().unwrap_or_else(|_| gone());
            };
            crossbeam_channel::select! {
                recv(self.events) -> event => return event.unwrap_or_else(|_| gone()),
                recv(accepted) -> reply => {
                    self.accepted = None;
                    if let Some(message) = reply.ok().and_then(refusal) {
                        return EngineEvent::Failed(message);
                    }
                }
            }
        }
    }

    fn observe(&mut self, event: &EngineEvent) {
        self.carried.locked().observe(&self.watch, event);
        self.done = event.is_terminal();
    }

    pub fn answer(&self, ask: AskId, decision: Decision) -> Result<(), Error> {
        self.control().answer(ask, decision)
    }

    pub fn control(&self) -> RunControl {
        RunControl { id: self.id, dispatcher: Arc::clone(&self.dispatcher) }
    }
}

#[derive(Debug, Clone)]
pub struct RunControl {
    id: RunId,
    dispatcher: Arc<Dispatcher>,
}

impl RunControl {
    pub fn answer(&self, ask: AskId, decision: Decision) -> Result<(), Error> {
        self.dispatcher.send(ClientCommand::AnswerAsk { run: self.id, ask, decision })
    }

    pub fn cancel(&self) -> Result<(), Error> {
        self.dispatcher.send(ClientCommand::CancelRun { run: self.id })
    }
}

impl Iterator for Run {
    type Item = EngineEvent;

    fn next(&mut self) -> Option<EngineEvent> {
        self.next_event()
    }
}

impl Drop for Run {
    fn drop(&mut self) {
        if self.done {
            return;
        }
        self.dispatcher.forget(self.id);
        let _ = self.dispatcher.send(ClientCommand::CancelRun { run: self.id });
    }
}

fn gone() -> EngineEvent {
    EngineEvent::Failed(BACKEND_GONE.into())
}

fn refusal(reply: Reply) -> Option<String> {
    match reply {
        Reply::Error { message } => Some(message),
        _ => None,
    }
}

#[cfg(test)]
#[path = "run_tests.rs"]
mod tests;
