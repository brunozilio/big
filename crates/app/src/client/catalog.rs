use std::path::Path;

use crate::api::{ClientCommand, Reply};
use domain::SessionId;
use domain::catalog::Listed;
use domain::trace::{HistoryStamp, Record};

use crate::{Client, Error};

impl Client {
    pub fn sessions(&self) -> Result<Vec<Listed>, Error> {
        self.dispatcher.call(ClientCommand::ListSessions, |reply| match reply {
            Reply::Sessions(listed) => Some(Ok(listed)),
            _ => None,
        })
    }

    pub fn history(&self, session: &SessionId, cwd: &Path) -> Result<Vec<Record>, Error> {
        let command = ClientCommand::SessionHistory { session: session.clone(), cwd: cwd.to_path_buf() };
        self.dispatcher.call(command, |reply| match reply {
            Reply::History(records) => Some(Ok(records)),
            _ => None,
        })
    }

    pub fn history_since(
        &self,
        session: &SessionId,
        cwd: &Path,
        known: Option<HistoryStamp>,
    ) -> Result<(Option<HistoryStamp>, Option<Vec<Record>>), Error> {
        let command = ClientCommand::SessionHistorySince { session: session.clone(), cwd: cwd.to_path_buf(), known };
        self.dispatcher.call(command, |reply| match reply {
            Reply::HistorySince { stamp, records } => Some(Ok((stamp, records))),
            _ => None,
        })
    }
}
