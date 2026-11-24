use domain::{ChoreAnswer, Decision, EngineEvent, SessionCommand};

use crate::Error;
use crate::client::run::Run;
use crate::client::session::Session;

impl Session {
    pub fn side(&self, question: &str) -> Result<Run, Error> {
        self.chore(SessionCommand::Side, question)
    }

    pub fn suggest(&self) -> Result<Run, Error> {
        self.chore(SessionCommand::Suggest, "")
    }
}

impl Run {
    pub fn side_answer(self) -> Result<String, Error> {
        match self.chore_answer()? {
            Some(ChoreAnswer::Side(text)) => Ok(text),
            _ => Err(Error::Run("the side question got no answer".into())),
        }
    }

    pub fn chore_answer(mut self) -> Result<Option<ChoreAnswer>, Error> {
        let mut answer = None;
        let mut kind = domain::FailureKind::Other;
        while let Some(event) = self.next_event() {
            match event {
                EngineEvent::Chore(given) => answer = Some(given),
                EngineEvent::Asked(ask) => self.answer(ask.id, Decision::Deny)?,
                EngineEvent::Failure(given) => kind = given,
                EngineEvent::Failed(message) => return Err(domain::failure::RunFailure { kind, message }.into()),
                EngineEvent::Cancelled => return Err(Error::Run("cancelled".into())),
                _ => {}
            }
        }
        Ok(answer)
    }
}
