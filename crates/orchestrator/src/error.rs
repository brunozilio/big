#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Store(#[from] store::Error),
    #[error("there is no item {0}")]
    UnknownItem(String),
    #[error("the workflow could not be loaded: {0}")]
    Script(String),
    #[error("nothing in item {item} waits for a human at step {step}")]
    NotWaiting { item: String, step: String },
    #[error("there is no open question {0}")]
    UnknownAsk(String),
    #[error("the step {0} never started")]
    UnknownStep(String),
    #[error("the item {0} has ended")]
    Ended(String),
    #[error("no agent holds this cloud token")]
    UnknownCloudToken,
    #[error("the cloud credentials could not be had: {0}")]
    Cloud(String),
}

pub type Result<T> = std::result::Result<T, Error>;
