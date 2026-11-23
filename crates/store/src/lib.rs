mod applications;
mod branding;
mod connections;
mod error;
mod factory;
mod log;
mod pins;
mod schema;
mod sync;

use std::path::Path;
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use rusqlite::{Connection, Transaction, TransactionBehavior};

pub use applications::{Application, CodeChoice, Repository, TrackerChoice};
pub use connections::ConnectionRecord;
pub use error::Error;
pub use factory::{Generation, GenerationReport};

pub type Result<T> = std::result::Result<T, Error>;

const BUSY_WAIT: Duration = Duration::from_secs(10);

#[derive(Debug)]
pub struct Store {
    connection: Mutex<Connection>,
    snapshot_every: u64,
}

impl Store {
    pub fn open(path: &Path, snapshot_every: u64) -> Result<Self> {
        Self::ready(Connection::open(path)?, snapshot_every)
    }

    pub fn in_memory(snapshot_every: u64) -> Result<Self> {
        Self::ready(Connection::open_in_memory()?, snapshot_every)
    }

    fn ready(mut connection: Connection, snapshot_every: u64) -> Result<Self> {
        connection.busy_timeout(BUSY_WAIT)?;
        connection.pragma_update(None, "journal_mode", "WAL")?;
        connection.pragma_update(None, "synchronous", "NORMAL")?;
        schema::migrate(&mut connection)?;
        Ok(Self { connection: Mutex::new(connection), snapshot_every })
    }

    fn locked(&self) -> MutexGuard<'_, Connection> {
        self.connection.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn write<T>(&self, work: impl FnOnce(&Transaction<'_>) -> Result<T>) -> Result<T> {
        let mut connection = self.locked();
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let value = work(&transaction)?;
        transaction.commit()?;
        Ok(value)
    }
}

fn signed(number: u64) -> Result<i64> {
    i64::try_from(number).map_err(|_| Error::OutOfRange(number.to_string()))
}

fn unsigned(number: i64) -> Result<u64> {
    u64::try_from(number).map_err(|_| Error::OutOfRange(number.to_string()))
}
