use std::path::Path;
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use rusqlite::types::Value as Raw;
use rusqlite::{Connection, TransactionBehavior};

use crate::{Conn, Dialect, Result, Row, Value};

const WAIT: Duration = Duration::from_secs(10);

#[derive(Debug)]
pub struct Sqlite(Mutex<Connection>);

impl Sqlite {
    pub fn open(path: &Path) -> Result<Self> {
        Self::ready(Connection::open(path)?, true)
    }

    pub fn in_memory() -> Result<Self> {
        Self::ready(Connection::open_in_memory()?, false)
    }

    fn ready(connection: Connection, on_disk: bool) -> Result<Self> {
        connection.busy_timeout(WAIT)?;
        if on_disk {
            connection.pragma_update(None, "journal_mode", "WAL")?;
            connection.pragma_update(None, "synchronous", "NORMAL")?;
        }
        connection.pragma_update(None, "foreign_keys", "ON")?;
        Ok(Self(Mutex::new(connection)))
    }

    fn held(&self) -> MutexGuard<'_, Connection> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub fn with_raw<R>(&self, work: impl FnOnce(&Connection) -> R) -> R {
        work(&self.held())
    }

    pub fn read<R>(&self, work: impl FnOnce(&mut dyn Conn) -> Result<R>) -> Result<R> {
        let held = self.held();
        work(&mut Plain(&held))
    }

    pub fn write<R>(&self, work: impl FnOnce(&mut dyn Conn) -> Result<R>) -> Result<R> {
        let mut held = self.held();
        let transaction = held.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let done = work(&mut Plain(&transaction))?;
        transaction.commit()?;
        Ok(done)
    }
}

struct Plain<'a>(&'a Connection);

fn raw(value: &Value) -> Raw {
    match value {
        Value::Null => Raw::Null,
        Value::Integer(number) => Raw::Integer(*number),
        Value::Real(number) => Raw::Real(*number),
        Value::Text(text) => Raw::Text(text.clone()),
        Value::Blob(bytes) => Raw::Blob(bytes.clone()),
    }
}

fn cooked(value: Raw) -> Value {
    match value {
        Raw::Null => Value::Null,
        Raw::Integer(number) => Value::Integer(number),
        Raw::Real(number) => Value::Real(number),
        Raw::Text(text) => Value::Text(text),
        Raw::Blob(bytes) => Value::Blob(bytes),
    }
}

impl Conn for Plain<'_> {
    fn dialect(&self) -> Dialect {
        Dialect::Sqlite
    }

    fn execute(&mut self, sql: &str, args: &[Value]) -> Result<u64> {
        let changed = self.0.prepare(sql)?.execute(rusqlite::params_from_iter(args.iter().map(raw)))?;
        Ok(u64::try_from(changed).unwrap_or(u64::MAX))
    }

    fn query(&mut self, sql: &str, args: &[Value]) -> Result<Vec<Row>> {
        let mut statement = self.0.prepare(sql)?;
        let width = statement.column_count();
        let rows = statement.query_map(rusqlite::params_from_iter(args.iter().map(raw)), |row| {
            (0..width)
                .map(|index| row.get::<_, Raw>(index).map(cooked))
                .collect::<rusqlite::Result<Vec<Value>>>()
                .map(Row)
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<Row>>>()?)
    }

    fn batch(&mut self, sql: &str) -> Result<()> {
        Ok(self.0.execute_batch(sql)?)
    }
}
