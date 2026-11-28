mod copy;
mod lease;
mod postgres_db;
mod sql;
mod sqlite_db;
mod value;

use std::path::Path;

pub use copy::{Copied, copy_into};
pub use lease::{Lease, Turn};
pub use postgres_db::Password;
pub use sql::{for_postgres, schema_for_postgres};
pub use value::{FromValue, Param, Row, Value};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Sqlite(#[from] rusqlite::Error),
    #[error(transparent)]
    Postgres(#[from] postgres::Error),
    #[error("{0}")]
    Decode(String),
    #[error("{0}")]
    Setup(String),
}

impl Error {
    pub fn decode(message: impl Into<String>) -> Self {
        Self::Decode(message.into())
    }

    pub fn busy(&self) -> bool {
        matches!(self, Self::Sqlite(rusqlite::Error::SqliteFailure(failure, _))
            if matches!(failure.code, rusqlite::ErrorCode::DatabaseBusy | rusqlite::ErrorCode::DatabaseLocked))
    }

    pub fn conflict(&self) -> bool {
        match self {
            Self::Sqlite(rusqlite::Error::SqliteFailure(failure, _)) => {
                failure.code == rusqlite::ErrorCode::ConstraintViolation
            }
            Self::Postgres(error) => error.code().is_some_and(|code| code.code().starts_with("23")),
            _ => false,
        }
    }
}

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dialect {
    Sqlite,
    Postgres,
}

pub trait Conn {
    fn dialect(&self) -> Dialect;
    fn execute(&mut self, sql: &str, args: &[Value]) -> Result<u64>;
    fn query(&mut self, sql: &str, args: &[Value]) -> Result<Vec<Row>>;
    fn batch(&mut self, sql: &str) -> Result<()>;
}

impl dyn Conn + '_ {
    pub fn row(&mut self, sql: &str, args: &[Value]) -> Result<Option<Row>> {
        Ok(self.query(sql, args)?.into_iter().next())
    }

    pub fn one<T: FromValue>(&mut self, sql: &str, args: &[Value]) -> Result<Option<T>> {
        self.row(sql, args)?.map(|row| row.get(0)).transpose()
    }

    pub fn all<T: FromValue>(&mut self, sql: &str, args: &[Value]) -> Result<Vec<T>> {
        self.query(sql, args)?.iter().map(|row| row.get(0)).collect()
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Target {
    pub url: String,
    pub ca_file: String,
}

#[derive(Debug)]
enum Inner {
    Sqlite(sqlite_db::Sqlite),
    Postgres(Box<postgres_db::Postgres>),
}

#[derive(Debug)]
pub struct Database(Inner);

impl Database {
    pub fn sqlite(path: &Path) -> Result<Self> {
        Ok(Self(Inner::Sqlite(sqlite_db::Sqlite::open(path)?)))
    }

    pub fn sqlite_in_memory() -> Result<Self> {
        Ok(Self(Inner::Sqlite(sqlite_db::Sqlite::in_memory()?)))
    }

    pub fn postgres(target: &Target) -> Result<Self> {
        Ok(Self(Inner::Postgres(Box::new(postgres_db::Postgres::connect(target, None)?))))
    }

    pub fn postgres_with_password(target: &Target, password: Password) -> Result<Self> {
        Ok(Self(Inner::Postgres(Box::new(postgres_db::Postgres::connect(target, Some(password))?))))
    }

    pub fn dialect(&self) -> Dialect {
        match &self.0 {
            Inner::Sqlite(_) => Dialect::Sqlite,
            Inner::Postgres(_) => Dialect::Postgres,
        }
    }

    pub fn sqlite_connection<R>(&self, work: impl FnOnce(&rusqlite::Connection) -> R) -> Option<R> {
        match &self.0 {
            Inner::Sqlite(sqlite) => Some(sqlite.with_raw(work)),
            Inner::Postgres(_) => None,
        }
    }

    pub fn read<R: Send>(&self, work: impl FnOnce(&mut dyn Conn) -> Result<R> + Send) -> Result<R> {
        match &self.0 {
            Inner::Sqlite(sqlite) => sqlite.read(work),
            Inner::Postgres(postgres) => postgres.run(false, work),
        }
    }

    pub fn write<R: Send>(&self, work: impl FnOnce(&mut dyn Conn) -> Result<R> + Send) -> Result<R> {
        match &self.0 {
            Inner::Sqlite(sqlite) => sqlite.write(work),
            Inner::Postgres(postgres) => postgres.run(true, work),
        }
    }

    pub fn migrate(&self, table: &str, migrations: &[&str]) -> Result<usize> {
        let dialect = self.dialect();
        self.write(|conn| {
            conn.batch(&format!("CREATE TABLE IF NOT EXISTS {table} (version BIGINT NOT NULL)"))?;
            let current: i64 = conn.one(&format!("SELECT COUNT(*) FROM {table}"), &[])?.unwrap_or(0);
            let done = usize::try_from(current).unwrap_or(0);
            if done > migrations.len() {
                return Err(Error::Setup(format!(
                    "the database was written by a newer version ({done} steps in {table}, this one knows {})",
                    migrations.len()
                )));
            }
            for (version, migration) in migrations.iter().enumerate().skip(done) {
                let text = match dialect {
                    Dialect::Sqlite => (*migration).to_owned(),
                    Dialect::Postgres => schema_for_postgres(migration),
                };
                conn.batch(&text)?;
                conn.execute(&format!("INSERT INTO {table} (version) VALUES (?1)"), args![version + 1])?;
            }
            Ok(done)
        })
    }
}

#[cfg(test)]
mod tests;
