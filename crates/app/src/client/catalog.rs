use std::path::Path;

use crate::api::{ClientCommand, Reply};
use domain::SessionId;
use domain::catalog::Listed;
use domain::trace::Record;

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
}
