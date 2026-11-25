use crate::api::{BackgroundJob, ClientCommand, Reply};
use crate::{Client, Error};

impl Client {
    pub fn background_jobs(&self) -> Result<Vec<BackgroundJob>, Error> {
        self.dispatcher.call(ClientCommand::BackgroundJobs, |reply| match reply {
            Reply::Background(jobs) => Some(Ok(jobs)),
            _ => None,
        })
    }

    pub fn stop_background(&self, id: &str) -> Result<(), Error> {
        self.dispatcher.call(ClientCommand::StopBackground { id: id.to_owned() }, |reply| match reply {
            Reply::Done => Some(Ok(())),
            _ => None,
        })
    }
}
