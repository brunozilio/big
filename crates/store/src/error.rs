#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("database: {0}")]
    Database(#[from] database::Error),
    #[error("stored json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("number out of range: {0}")]
    OutOfRange(String),
}

impl Error {
    pub fn busy(&self) -> bool {
        matches!(self, Self::Database(error) if error.busy())
    }

    pub fn conflict(&self) -> bool {
        matches!(self, Self::Database(error) if error.conflict())
    }
}
