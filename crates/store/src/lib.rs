mod applications;
mod archive;
mod branding;
mod connections;
mod connector_memory;
mod error;
mod factory;
mod identity;
mod log;
pub mod ownership;
mod preferences;
mod retention;
mod schema;
mod sync;

use std::ops::{Deref, DerefMut};
use std::path::Path;
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use rusqlite::{Connection, Transaction, TransactionBehavior};

pub use applications::{Application, Binding, Bindings, Choice, Environment, Fields, Repository, watched};
pub use connections::ConnectionRecord;
pub use error::Error;
pub use factory::Generation;
pub use identity::{CodeCheck, Passkey, Person};
pub use ownership::OwnedIds;
pub use retention::Forgotten;

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

    fn locked(&self) -> Held<'_> {
        let asked = Instant::now();
        let guard = self.connection.lock().unwrap_or_else(PoisonError::into_inner);
        Held { guard, waited: asked.elapsed(), since: Instant::now() }
    }

    fn write<T>(&self, work: impl FnOnce(&Transaction<'_>) -> Result<T>) -> Result<T> {
        let mut connection = self.locked();
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let value = work(&transaction)?;
        transaction.commit()?;
        Ok(value)
    }
}

struct Held<'a> {
    guard: MutexGuard<'a, Connection>,
    waited: Duration,
    since: Instant,
}

impl Deref for Held<'_> {
    type Target = Connection;

    fn deref(&self) -> &Connection {
        &self.guard
    }
}

impl DerefMut for Held<'_> {
    fn deref_mut(&mut self) -> &mut Connection {
        &mut self.guard
    }
}

impl Drop for Held<'_> {
    fn drop(&mut self) {
        tracing::event!(
            target: "timing", tracing::Level::DEBUG,
            timing = "store.query",
            waited_us = micros(self.waited),
            held_us = micros(self.since.elapsed()),
            "timing"
        );
    }
}

fn micros(spent: Duration) -> u64 {
    u64::try_from(spent.as_micros()).unwrap_or(u64::MAX)
}

fn signed(number: u64) -> Result<i64> {
    i64::try_from(number).map_err(|_| Error::OutOfRange(number.to_string()))
}

fn unsigned(number: i64) -> Result<u64> {
    u64::try_from(number).map_err(|_| Error::OutOfRange(number.to_string()))
}
