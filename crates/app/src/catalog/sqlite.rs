use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use database::{Conn, args};
use domain::SessionId;
use domain::catalog::{Catalog, KEPT, Listed};

use super::SessionCatalog;
use crate::db::codec::{counted, name, named, stored};
use crate::db::{Db, alone};

const COLUMNS: &str = "session, cwd, title, created, updated, status, unread, tie";
const UPSERT: &str = "INSERT INTO sessions (session, cwd, title, created, updated, status, unread, tie) \
     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8) \
     ON CONFLICT (session) DO UPDATE SET cwd = excluded.cwd, title = excluded.title, created = excluded.created, \
     updated = excluded.updated, status = excluded.status, unread = excluded.unread, \
     tie = excluded.tie";

#[derive(Debug, Clone)]
pub struct SqliteSessionCatalog {
    db: Db,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Row {
    listed: Listed,
    tie: i64,
}

impl SqliteSessionCatalog {
    pub fn new(db: Db) -> Self {
        Self { db }
    }
}

impl SessionCatalog for SqliteSessionCatalog {
    fn read(&self) -> Catalog {
        match self.db.read(rows) {
            Ok(rows) => catalog_of(&rows),
            Err(error) => {
                tracing::warn!(%error, "the session catalog could not be read; going on from nothing");
                Catalog::default()
            }
        }
    }

    fn update(&self, change: &mut (dyn FnMut(&mut Catalog) + Send)) {
        let changed = self.db.write(|conn| {
            alone(conn, "sessions")?;
            let before = rows(conn)?;
            let mut catalog = catalog_of(&before);
            change(&mut catalog);
            keep(conn, &before, &catalog)
        });
        if let Err(error) = changed {
            tracing::warn!(%error, "could not keep the session catalog; it was left as it was");
        }
    }

    fn is_empty(&self) -> bool {
        self.db.read(is_empty).unwrap_or_else(|error| {
            tracing::warn!(%error, "the session catalog could not be read; taking it for empty");
            true
        })
    }
}

pub(super) fn is_empty(conn: &mut dyn Conn) -> database::Result<bool> {
    Ok(conn.one("SELECT NOT EXISTS (SELECT 1 FROM sessions)", &[])?.unwrap_or(true))
}

fn rows(conn: &mut dyn Conn) -> database::Result<Vec<Row>> {
    let sql = format!("SELECT {COLUMNS} FROM sessions ORDER BY updated DESC, tie LIMIT ?1");
    let found = conn.query(&sql, args![stored(KEPT as u64)])?;
    found
        .iter()
        .map(|row| {
            Ok(Row {
                listed: Listed {
                    session: SessionId(row.get(0)?),
                    cwd: PathBuf::from(row.get::<String>(1)?),
                    title: row.get(2)?,
                    created: counted(row.get(3)?),
                    updated: counted(row.get(4)?),
                    status: named(5, row.get(5)?)?,
                    unread: row.get(6)?,
                },
                tie: row.get(7)?,
            })
        })
        .collect()
}

fn catalog_of(rows: &[Row]) -> Catalog {
    let listed: Vec<&Listed> = rows.iter().map(|row| &row.listed).collect();
    serde_json::to_value(listed).and_then(serde_json::from_value).unwrap_or_else(|error| {
        tracing::warn!(%error, "the session catalog could not be rebuilt from its rows");
        Catalog::default()
    })
}

fn ranked(catalog: &Catalog, had: &HashMap<&str, &Row>) -> Vec<Row> {
    let mut last_at: HashMap<u64, i64> = HashMap::new();
    let mut after_the_last = |listed: &Listed| {
        let had = had.get(listed.session.0.as_str()).map_or(0, |row| row.tie);
        let tie = last_at.get(&listed.updated).map_or(had, |last| had.max(last + 1));
        last_at.insert(listed.updated, tie);
        tie
    };
    catalog.listed().iter().map(|listed| Row { tie: after_the_last(listed), listed: listed.clone() }).collect()
}

pub(super) fn keep(conn: &mut dyn Conn, before: &[Row], catalog: &Catalog) -> database::Result<()> {
    let had: HashMap<&str, &Row> = before.iter().map(|row| (row.listed.session.0.as_str(), row)).collect();
    let wanted = ranked(catalog, &had);
    let listed: HashSet<&str> = wanted.iter().map(|row| row.listed.session.0.as_str()).collect();
    for row in wanted.iter().filter(|row| had.get(row.listed.session.0.as_str()).copied() != Some(*row)) {
        let Listed { session, cwd, title, created, updated, status, unread } = &row.listed;
        conn.execute(
            UPSERT,
            args![session.0, text(cwd), title, stored(*created), stored(*updated), name(status)?, unread, row.tie],
        )?;
    }
    for gone in before.iter().filter(|row| !listed.contains(row.listed.session.0.as_str())) {
        conn.execute("DELETE FROM sessions WHERE session = ?1", args![gone.listed.session.0])?;
    }
    Ok(())
}

fn text(cwd: &Path) -> String {
    cwd.to_string_lossy().into_owned()
}
