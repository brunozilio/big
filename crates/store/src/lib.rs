mod applications;
mod archive;
mod audit;
mod audit_chain;
mod audit_delivery;
mod audit_row;
mod branding;
mod browser_devices;
mod connections;
mod connector_calls;
mod connector_memory;
mod error;
mod factory;
mod identity;
mod legacy;
mod log;
pub mod ownership;
mod passwords;
mod preferences;
mod retention;
mod roles;
mod schema;
mod spend;
mod sync;

use std::path::Path;
use std::time::{Duration, Instant};

use database::{Conn, Database};

pub use applications::{Application, Binding, Bindings, Choice, Environment, Fields, Repository, watched};
pub use audit::{AuditFacets, AuditFilter, AuditPage, AuditRecord};
pub use audit_chain::{AuditVerified, GENESIS};
pub use audit_delivery::AuditDelivery;
pub use browser_devices::BrowserDevice;
pub use connections::ConnectionRecord;
pub use connector_calls::{ConnectorCall, ConnectorCalls};
pub use error::Error;
pub use factory::Generation;
pub use identity::{CodeCheck, Passkey, Person};
pub use ownership::OwnedIds;
pub use retention::Forgotten;
pub use roles::HeldRole;
pub use spend::Spent;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug)]
pub struct Store {
    database: Database,
    snapshot_every: u64,
}

impl Store {
    pub fn open(path: &Path, snapshot_every: u64) -> Result<Self> {
        Self::with_database(Database::sqlite(path)?, snapshot_every)
    }

    pub fn in_memory(snapshot_every: u64) -> Result<Self> {
        Self::with_database(Database::sqlite_in_memory()?, snapshot_every)
    }

    pub fn with_database(database: Database, snapshot_every: u64) -> Result<Self> {
        schema::migrate(&database)?;
        Ok(Self { database, snapshot_every })
    }

    pub fn database(&self) -> &Database {
        &self.database
    }

    fn read<T: Send>(&self, work: impl FnOnce(&mut dyn Conn) -> Result<T> + Send) -> Result<T> {
        self.run(false, work)
    }

    fn write<T: Send>(&self, work: impl FnOnce(&mut dyn Conn) -> Result<T> + Send) -> Result<T> {
        self.run(true, work)
    }

    fn run<T: Send>(&self, writes: bool, work: impl FnOnce(&mut dyn Conn) -> Result<T> + Send) -> Result<T> {
        let since = Instant::now();
        let mut kept = None;
        let guarded = |conn: &mut dyn Conn| {
            work(conn).map_err(|error| match error {
                Error::Database(inner) => inner,
                other => {
                    let shown = other.to_string();
                    kept = Some(other);
                    database::Error::decode(shown)
                }
            })
        };
        let done = if writes { self.database.write(guarded) } else { self.database.read(guarded) };
        tracing::event!(
            target: "timing", tracing::Level::DEBUG,
            timing = "store.query",
            held_us = micros(since.elapsed()),
            "timing"
        );
        match kept {
            Some(error) => Err(error),
            None => done.map_err(Error::from),
        }
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
