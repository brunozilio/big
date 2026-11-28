use std::path::{Path, PathBuf};

use agent_engine::sessions::scoped_id;
use database::args;
use domain::SessionId;
use rusqlite::{Connection, OpenFlags};

use crate::db::Db;

const INSERT: &str = "INSERT INTO sessions (session, cwd, title, created, updated, status, archived, unread, tie) \
     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9) ON CONFLICT DO NOTHING";
const SELECT: &str = "SELECT session, cwd, title, created, updated, status, archived, unread, tie FROM sessions";

#[derive(Debug, Clone, PartialEq, Eq)]
struct Row {
    session: String,
    cwd: String,
    title: Option<String>,
    created: i64,
    updated: i64,
    status: String,
    archived: i64,
    unread: i64,
    tie: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Former {
    pub name: String,
    pub database: PathBuf,
    pub sessions: PathBuf,
}

fn files_of(stem: &str) -> [String; 5] {
    [
        format!("{stem}.jsonl"),
        format!("{stem}.trace.jsonl"),
        format!("{stem}.json"),
        format!("checkpoints-{stem}.json"),
        format!("snapshot-{stem}.json"),
    ]
}

fn rows(database: &Path) -> rusqlite::Result<Vec<Row>> {
    let connection = Connection::open_with_flags(database, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    let mut all = connection.prepare(SELECT)?;
    let rows = all.query_map([], |row| {
        Ok(Row {
            session: row.get(0)?,
            cwd: row.get(1)?,
            title: row.get(2)?,
            created: row.get(3)?,
            updated: row.get(4)?,
            status: row.get(5)?,
            archived: row.get(6)?,
            unread: row.get(7)?,
            tie: row.get(8)?,
        })
    })?;
    rows.collect()
}

fn copied(row: &Row, from: &Path, to: &Path) -> std::io::Result<bool> {
    let stem = scoped_id(Path::new(&row.cwd), &SessionId(row.session.clone())).0;
    let mut kept = false;
    for name in files_of(&stem) {
        let (source, target) = (from.join(&name), to.join(&name));
        if !source.is_file() {
            continue;
        }
        if !target.exists() {
            std::fs::create_dir_all(to)?;
            std::fs::copy(&source, &target)?;
        }
        kept |= name.ends_with(".jsonl") || name == format!("{stem}.json");
    }
    Ok(kept)
}

fn marker(data: &Path, former: &Former) -> PathBuf {
    let plain: String =
        former.name.chars().map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '-' }).collect();
    data.join(format!("imported-from-{plain}"))
}

pub(crate) fn import(db: &Db, data: &Path, sessions: &Path, formers: &[Former]) {
    for former in formers.iter().filter(|former| former.database.is_file()) {
        let done = marker(data, former);
        if done.exists() {
            continue;
        }
        match import_one(db, sessions, former) {
            Ok(count) => {
                tracing::info!(former = %former.name, sessions = count, "the sessions of a former name were imported");
                if let Err(error) = std::fs::write(&done, count.to_string()) {
                    tracing::warn!(%error, "the import of a former name could not be marked as done");
                }
            }
            Err(error) => {
                tracing::warn!(%error, former = %former.name, "the sessions of a former name were not imported")
            }
        }
    }
}

fn import_one(db: &Db, sessions: &Path, former: &Former) -> Result<usize, String> {
    let rows = rows(&former.database).map_err(|error| error.to_string())?;
    let mut kept = Vec::new();
    for row in rows {
        if copied(&row, &former.sessions, sessions).map_err(|error| error.to_string())? {
            kept.push(row);
        }
    }
    db.write(|conn| {
        let mut added = 0;
        for row in &kept {
            added += conn.execute(
                INSERT,
                args![
                    row.session,
                    row.cwd,
                    row.title,
                    row.created,
                    row.updated,
                    row.status,
                    row.archived,
                    row.unread,
                    row.tie
                ],
            )?;
        }
        Ok(usize::try_from(added).unwrap_or(usize::MAX))
    })
    .map_err(|error| error.to_string())
}

#[cfg(test)]
#[path = "former_tests.rs"]
mod tests;
