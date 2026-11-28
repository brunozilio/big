#![allow(dead_code, reason = "each test file uses its own part of the helpers")]

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use database::{Database, Target};
use store::Store;

pub const POSTGRES: &str = "DATABASE_TEST_URL";

pub fn postgres_ready() -> bool {
    std::env::var_os(POSTGRES).is_some_and(|url| !url.is_empty())
}

#[derive(Debug)]
pub enum Place {
    File { _folder: tempfile::TempDir, path: PathBuf },
    Postgres { admin: String, schema: String },
}

fn admin_run(admin: &str, sql: &str) -> database::Result<()> {
    let db = Database::postgres(&Target { url: admin.to_owned(), ca_file: String::new() })?;
    db.write(|conn| conn.batch(sql))
}

fn fresh_schema() -> String {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |elapsed| elapsed.as_nanos());
    format!("t_{}_{}_{}", std::process::id(), nanos, NEXT.fetch_add(1, Ordering::Relaxed))
}

impl Place {
    pub fn sqlite() -> Place {
        let folder = tempfile::tempdir().expect("a folder");
        let path = folder.path().join("app.db");
        Place::File { _folder: folder, path }
    }

    pub fn postgres() -> Place {
        let admin = std::env::var(POSTGRES).expect("DATABASE_TEST_URL names a PostgreSQL");
        let schema = fresh_schema();
        admin_run(&admin, &format!("CREATE SCHEMA {schema}")).expect("the test schema is created");
        Place::Postgres { admin, schema }
    }

    pub fn database(&self) -> Database {
        match self {
            Place::File { path, .. } => Database::sqlite(path).expect("the file opens"),
            Place::Postgres { admin, schema } => {
                let joint = if admin.contains('?') { '&' } else { '?' };
                let url = format!("{admin}{joint}options=-csearch_path%3D{schema}");
                Database::postgres(&Target { url, ca_file: String::new() }).expect("the database answers")
            }
        }
    }

    pub fn open(&self, snapshot_every: u64) -> Store {
        Store::with_database(self.database(), snapshot_every).expect("the store answers")
    }

    pub fn raw(&self, sql: &str, args: &[database::Value]) -> u64 {
        self.database().write(|conn| conn.execute(sql, args)).expect("the raw statement runs")
    }
}

impl Drop for Place {
    fn drop(&mut self) {
        if let Place::Postgres { admin, schema } = self {
            let _ = admin_run(admin, &format!("DROP SCHEMA IF EXISTS {schema} CASCADE"));
        }
    }
}

pub type Fresh = fn() -> Place;

macro_rules! scenarios {
    ($($name:ident),+ $(,)?) => {
        mod on_sqlite {
            $(
                #[test]
                fn $name() {
                    super::$name(crate::common::Place::sqlite);
                }
            )+
        }

        mod on_postgres {
            $(
                #[test]
                #[ignore = "runs against the PostgreSQL named by DATABASE_TEST_URL"]
                fn $name() {
                    if crate::common::postgres_ready() {
                        super::$name(crate::common::Place::postgres);
                    } else {
                        eprintln!("skipped: {} is not set", crate::common::POSTGRES);
                    }
                }
            )+
        }
    };
}
