use std::path::{Path, PathBuf};
use std::time::Duration;

use database::{Database, Target};

use super::{Db, Error, WAIT, schema};

const URL: &str = "DATABASE_TEST_URL";

pub(crate) enum Place {
    File { _dir: tempfile::TempDir, file: PathBuf },
    Postgres { base: String, schema: String },
}

fn admin(base: &str) -> Database {
    Database::postgres(&Target { url: base.to_owned(), ca_file: String::new() }).expect("the test server")
}

impl Place {
    pub(crate) fn file() -> Self {
        let dir = tempfile::tempdir().expect("dir");
        let file = dir.path().join(super::FILE);
        Self::File { _dir: dir, file }
    }

    pub(crate) fn postgres() -> Option<Self> {
        let Some(base) = platform::environment::var(URL).filter(|url| !url.trim().is_empty()) else {
            eprintln!("{URL} is not set; the PostgreSQL scenario was skipped");
            return None;
        };
        let schema = format!("t_{}", uuid::Uuid::new_v4().simple());
        admin(&base).write(|conn| conn.batch(&format!("CREATE SCHEMA {schema}"))).expect("a schema of its own");
        Some(Self::Postgres { base, schema })
    }

    pub(crate) fn file_path(&self) -> Option<&Path> {
        match self {
            Self::File { file, .. } => Some(file),
            Self::Postgres { .. } => None,
        }
    }

    pub(crate) fn opened(&self, wait: Duration, migrations: &[&str]) -> Result<Db, Error> {
        match self {
            Self::File { file, .. } => Db::opened(file, wait, migrations),
            Self::Postgres { base, schema } => {
                let joiner = if base.contains('?') { '&' } else { '?' };
                let url = format!("{base}{joiner}options=-csearch_path%3D{schema}");
                let database = Database::postgres(&Target { url, ca_file: String::new() }).expect("connect");
                Db::given(database, migrations)
            }
        }
    }

    pub(crate) fn open(&self) -> Db {
        self.opened(WAIT, schema::MIGRATIONS).expect("open")
    }

    pub(crate) fn open_at(&self, version: usize) -> Db {
        self.opened(WAIT, &schema::MIGRATIONS[..version]).expect("open at a version")
    }
}

pub(crate) fn rows_written(db: &Db) -> u64 {
    db.kept.database.sqlite_connection(|connection| connection.total_changes()).expect("only SQLite counts them")
}

impl Drop for Place {
    fn drop(&mut self) {
        if let Self::Postgres { base, schema } = self {
            let dropped = admin(base).write(|conn| conn.batch(&format!("DROP SCHEMA IF EXISTS {schema} CASCADE")));
            if let Err(error) = dropped {
                eprintln!("the test schema {schema} was left behind: {error}");
            }
        }
    }
}

macro_rules! on_postgres {
    ($($name:ident => $scenario:path),* $(,)?) => {$(
        #[test]
        #[ignore = "needs DATABASE_TEST_URL"]
        fn $name() {
            if let Some(place) = $crate::db::testing::Place::postgres() {
                $scenario(&place);
            }
        }
    )*};
}

pub(crate) use on_postgres;
