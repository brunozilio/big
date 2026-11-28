use std::io;
use std::time::Duration;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{place}: {source}")]
    Database {
        place: String,
        #[source]
        source: database::Error,
    },
    #[error("{place} is being written by another process; gave up after {} s", waited.as_secs_f32())]
    Busy { place: String, waited: Duration },
    #[error(
        "{place} was written by a newer version (schema {found}, this version knows up to {known}); it was left as it is"
    )]
    Newer { place: String, found: i64, known: i64 },
    #[error("{place}: {source}")]
    Io {
        place: String,
        #[source]
        source: io::Error,
    },
}

impl Error {
    pub(super) fn of(place: &str, waited: Duration, source: database::Error) -> Self {
        if source.busy() {
            return Self::Busy { place: place.to_owned(), waited };
        }
        Self::Database { place: place.to_owned(), source }
    }
}

impl From<Error> for io::Error {
    fn from(error: Error) -> Self {
        let kind = match &error {
            Error::Io { source, .. } => source.kind(),
            Error::Busy { .. } => io::ErrorKind::TimedOut,
            Error::Database { .. } | Error::Newer { .. } => io::ErrorKind::InvalidData,
        };
        io::Error::new(kind, error.to_string())
    }
}
