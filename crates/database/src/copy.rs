use std::collections::{BTreeMap, BTreeSet};

use crate::{Conn, Database, Dialect, Error, Result, args};

const MARKER: &str = "copied_from_sqlite";

type Tables = BTreeMap<String, (Vec<String>, BTreeSet<String>)>;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Copied {
    pub tables: usize,
    pub rows: u64,
}

fn tables_of(source: &Database) -> Result<Tables> {
    source.read(|conn| {
        let names: Vec<String> = conn.all(
            "SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%' ORDER BY name",
            &[],
        )?;
        let mut found = BTreeMap::new();
        for name in names {
            let columns: Vec<String> = conn
                .query(&format!("PRAGMA table_info(\"{name}\")"), &[])?
                .iter()
                .map(|row| row.get(1))
                .collect::<Result<_>>()?;
            let parents: BTreeSet<String> = conn
                .query(&format!("PRAGMA foreign_key_list(\"{name}\")"), &[])?
                .iter()
                .map(|row| row.get(2))
                .collect::<Result<_>>()?;
            found.insert(name, (columns, parents));
        }
        Ok(found)
    })
}

fn ordered(tables: &Tables) -> Vec<String> {
    let mut done: Vec<String> = Vec::new();
    while done.len() < tables.len() {
        let before = done.len();
        for (name, (_, parents)) in tables {
            let ready =
                parents.iter().all(|parent| parent == name || done.contains(parent) || !tables.contains_key(parent));
            if !done.contains(name) && ready {
                done.push(name.clone());
            }
        }
        if done.len() == before {
            done.extend(tables.keys().filter(|name| !done.contains(name)).cloned().collect::<Vec<_>>());
        }
    }
    done
}

fn target_columns(conn: &mut dyn Conn, table: &str) -> Result<Vec<String>> {
    conn.all(
        "SELECT column_name FROM information_schema.columns WHERE table_schema = current_schema() AND table_name = ?1",
        args![table],
    )
}

fn copied_table(source: &Database, conn: &mut dyn Conn, table: &str, columns: &[String]) -> Result<u64> {
    let present = target_columns(conn, table)?;
    let shared: Vec<&String> = columns.iter().filter(|column| present.contains(column)).collect();
    if shared.is_empty() || conn.one::<i64>(&format!("SELECT COUNT(*) FROM \"{table}\""), &[])?.unwrap_or(0) > 0 {
        return Ok(0);
    }
    let listed = shared.iter().map(|column| format!("\"{column}\"")).collect::<Vec<_>>().join(", ");
    let rows = source.read(|source| source.query(&format!("SELECT {listed} FROM \"{table}\""), &[]))?;
    let slots = (1..=shared.len()).map(|index| format!("?{index}")).collect::<Vec<_>>().join(", ");
    let insert = format!("INSERT INTO \"{table}\" ({listed}) VALUES ({slots})");
    let mut count = 0;
    for row in rows {
        count += conn.execute(&insert, &row.0)?;
    }
    Ok(count)
}

pub fn copy_into(source: &Database, target: &Database, skip: &[&str]) -> Result<Option<Copied>> {
    if source.dialect() != Dialect::Sqlite || target.dialect() != Dialect::Postgres {
        return Err(Error::Setup("copying goes from SQLite to PostgreSQL".to_owned()));
    }
    let tables = tables_of(source)?;
    target.write(|conn| {
        conn.batch(&format!("CREATE TABLE IF NOT EXISTS {MARKER} (at BIGINT NOT NULL)"))?;
        if conn.one::<i64>(&format!("SELECT COUNT(*) FROM {MARKER}"), &[])?.unwrap_or(0) > 0 {
            return Ok(None);
        }
        let mut copied = Copied::default();
        for table in ordered(&tables) {
            let Some((columns, _)) = tables.get(&table) else { continue };
            if skip.contains(&table.as_str()) {
                continue;
            }
            let rows = copied_table(source, conn, &table, columns)?;
            copied.tables += usize::from(rows > 0);
            copied.rows += rows;
        }
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |since| since.as_secs());
        conn.execute(&format!("INSERT INTO {MARKER} (at) VALUES (?1)"), args![now])?;
        Ok(Some(copied))
    })
}
