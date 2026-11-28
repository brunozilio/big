pub(crate) mod codec;
mod error;
pub(crate) mod imported;
mod schema;
#[cfg(test)]
pub(crate) mod testing;
#[cfg(test)]
mod tests;

use std::fs::OpenOptions;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use database::{Conn, Database, Dialect, args};

pub use error::Error;

pub const FILE: &str = "app.db";
pub const WAIT: Duration = Duration::from_secs(10);
const RETRY_EVERY: Duration = Duration::from_millis(10);
const VERSIONS: &str = "schema_version";
const MIGRATING: i64 = 0x6170_705f_6462;

#[derive(Clone)]
pub struct Db {
    kept: Arc<Kept>,
}

struct Kept {
    database: Database,
    place: String,
    wait: Duration,
}

impl std::fmt::Debug for Db {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Db").field("place", &self.kept.place).finish_non_exhaustive()
    }
}

impl Db {
    pub fn open(path: &Path) -> Result<Self, Error> {
        Self::opened(path, WAIT, schema::MIGRATIONS)
    }

    pub fn in_memory() -> Result<Self, Error> {
        let place = "the database in memory".to_owned();
        let database = Database::sqlite_in_memory().map_err(|source| Error::of(&place, WAIT, source))?;
        Self::ready(Kept { database, place, wait: WAIT }, schema::MIGRATIONS)
    }

    pub fn with_database(database: Database) -> Result<Self, Error> {
        Self::given(database, schema::MIGRATIONS)
    }

    pub(crate) fn given(database: Database, migrations: &[&str]) -> Result<Self, Error> {
        let place = match database.dialect() {
            Dialect::Sqlite => "the database",
            Dialect::Postgres => "the PostgreSQL database",
        };
        Self::ready(Kept { database, place: place.to_owned(), wait: WAIT }, migrations)
    }

    pub(crate) fn opened(path: &Path, wait: Duration, migrations: &[&str]) -> Result<Self, Error> {
        let place = path.display().to_string();
        made_private(path).map_err(|source| Error::Io { place: place.clone(), source })?;
        let database = settled(wait, || Database::sqlite(path)).map_err(|source| Error::of(&place, wait, source))?;
        let timed = database.sqlite_connection(|connection| connection.busy_timeout(wait));
        if let Some(Err(source)) = timed {
            return Err(Error::of(&place, wait, source.into()));
        }
        Self::ready(Kept { database, place, wait }, migrations)
    }

    fn ready(kept: Kept, migrations: &[&str]) -> Result<Self, Error> {
        let db = Self { kept: Arc::new(kept) };
        db.migrated(migrations)?;
        Ok(db)
    }

    fn migrated(&self, migrations: &[&str]) -> Result<(), Error> {
        let known = i64::try_from(migrations.len()).unwrap_or(i64::MAX);
        let count = format!("SELECT COUNT(*) FROM {VERSIONS}");
        let found = match self.read(|conn| conn.one::<i64>(&count, &[])) {
            Ok(found) if found.unwrap_or(0) >= known => found.unwrap_or(0),
            _ => self.write(|conn| migrate(conn, migrations))?,
        };
        if found > known {
            return Err(Error::Newer { place: self.kept.place.clone(), found, known });
        }
        Ok(())
    }

    pub(crate) fn read<R: Send>(
        &self,
        query: impl FnOnce(&mut dyn Conn) -> database::Result<R> + Send,
    ) -> Result<R, Error> {
        let snapshot = self.kept.database.read(|conn| {
            let begin = match conn.dialect() {
                Dialect::Sqlite => "BEGIN",
                Dialect::Postgres => "BEGIN ISOLATION LEVEL REPEATABLE READ READ ONLY",
            };
            conn.batch(begin)?;
            let answer = query(conn);
            let ended = conn.batch(if answer.is_ok() { "COMMIT" } else { "ROLLBACK" });
            let answer = answer?;
            ended.map(|()| answer)
        });
        snapshot.map_err(|source| self.error(source))
    }

    pub(crate) fn write<R: Send>(
        &self,
        change: impl FnOnce(&mut dyn Conn) -> database::Result<R> + Send,
    ) -> Result<R, Error> {
        self.kept.database.write(change).map_err(|source| self.error(source))
    }

    fn error(&self, source: database::Error) -> Error {
        Error::of(&self.kept.place, self.kept.wait, source)
    }
}

fn migrate(conn: &mut dyn Conn, migrations: &[&str]) -> database::Result<i64> {
    if conn.dialect() == Dialect::Postgres {
        conn.batch(&format!("SELECT pg_advisory_xact_lock({MIGRATING})"))?;
    }
    conn.batch(&format!("CREATE TABLE IF NOT EXISTS {VERSIONS} (version BIGINT NOT NULL)"))?;
    let found = conn.one::<i64>(&format!("SELECT COUNT(*) FROM {VERSIONS}"), &[])?.unwrap_or(0);
    let done = usize::try_from(found).unwrap_or(usize::MAX);
    for (step, migration) in migrations.iter().enumerate().skip(done) {
        match conn.dialect() {
            Dialect::Sqlite => conn.batch(migration)?,
            Dialect::Postgres => conn.batch(&database::schema_for_postgres(migration))?,
        }
        conn.execute(&format!("INSERT INTO {VERSIONS} (version) VALUES (?1)"), args![step + 1])?;
    }
    Ok(found)
}

pub(crate) fn alone(conn: &mut dyn Conn, table: &str) -> database::Result<()> {
    match conn.dialect() {
        Dialect::Sqlite => Ok(()),
        Dialect::Postgres => conn.batch(&format!("LOCK TABLE {table} IN EXCLUSIVE MODE")),
    }
}

fn made_private(path: &Path) -> std::io::Result<()> {
    if path.exists() {
        return Ok(());
    }
    if let Some(folder) = path.parent() {
        std::fs::create_dir_all(folder)?;
    }
    platform::private_file::open(OpenOptions::new().write(true).create(true).truncate(false), path).map(drop)
}

fn settled<T>(wait: Duration, mut attempt: impl FnMut() -> database::Result<T>) -> database::Result<T> {
    let give_up = Instant::now() + wait;
    loop {
        match attempt() {
            Err(error) if error.busy() && Instant::now() < give_up => std::thread::sleep(RETRY_EVERY),
            settled => return settled,
        }
    }
}
