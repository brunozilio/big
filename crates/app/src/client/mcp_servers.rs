use std::path::Path;

use crate::api::{ClientCommand, Reply};

use crate::{Client, Error};

impl Client {
    pub fn notices(&self, for_agent: bool) -> Result<crate::api::Notices, Error> {
        self.dispatcher.call(ClientCommand::TakeNotices { for_agent }, |reply| match reply {
            Reply::Notices(notices) => Some(Ok(notices)),
            _ => None,
        })
    }

    pub fn answer_teammate(&self, ask: domain::team::TeammateAsk, decision: domain::Decision) -> Result<(), Error> {
        self.dispatcher.call(ClientCommand::AnswerTeammate { ask, decision }, |reply| match reply {
            Reply::Done => Some(Ok(())),
            Reply::Error { message } => Some(Err(message)),
            _ => None,
        })
    }

    pub fn permissions(&self, cwd: &Path) -> Result<domain::permission::PermissionsInfo, Error> {
        self.dispatcher.call(ClientCommand::ListPermissions { path: cwd.to_path_buf() }, |reply| match reply {
            Reply::Permissions { permissions, .. } => Some(Ok(permissions)),
            _ => None,
        })
    }
}
