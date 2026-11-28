use std::collections::{HashMap, HashSet};

use database::{Conn, Row, args};
use domain::memory::scorecard::{Scorecard, Tally};
use domain::{Forgotten, Memory, MemoryBook, MemoryId};

use super::{MemoryStore, Revised};
use crate::db::codec::{from_json, json, name, named, small, stored};
use crate::db::{Db, alone};

const BOOK: &str = "SELECT next, revision, with_runs, with_corrections, with_cost_usd, without_runs, \
     without_corrections, without_cost_usd, briefed FROM memory_books WHERE project = ?1";
const KEEP_BOOK: &str = "INSERT INTO memory_books (project, next, revision, with_runs, with_corrections, \
     with_cost_usd, without_runs, without_corrections, without_cost_usd, briefed) \
     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10) \
     ON CONFLICT (project) DO UPDATE SET next = excluded.next, revision = excluded.revision, \
     with_runs = excluded.with_runs, with_corrections = excluded.with_corrections, \
     with_cost_usd = excluded.with_cost_usd, without_runs = excluded.without_runs, \
     without_corrections = excluded.without_corrections, without_cost_usd = excluded.without_cost_usd, \
     briefed = excluded.briefed";
const MEMORIES: &str = "SELECT id, forgotten_at, kind, author, text, tags, touches, history, created, origin, uses, \
     last_used, credit, position FROM memories WHERE project = ?1 ORDER BY forgotten_at IS NOT NULL, position";
const KEEP_MEMORY: &str = "INSERT INTO memories (project, id, position, forgotten_at, kind, author, text, tags, \
     touches, history, created, origin, uses, last_used, credit) \
     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15) \
     ON CONFLICT (project, id) DO UPDATE SET position = excluded.position, forgotten_at = excluded.forgotten_at, \
     kind = excluded.kind, author = excluded.author, text = excluded.text, tags = excluded.tags, \
     touches = excluded.touches, history = excluded.history, created = excluded.created, origin = excluded.origin, \
     uses = excluded.uses, last_used = excluded.last_used, credit = excluded.credit";

#[derive(Debug, Clone)]
pub struct SqliteMemoryStore {
    db: Db,
}

impl SqliteMemoryStore {
    pub fn new(db: Db) -> Self {
        Self { db }
    }
}

impl MemoryStore for SqliteMemoryStore {
    fn revision(&self, key: &str) -> u64 {
        self.db.read(|conn| revision(conn, key)).unwrap_or_else(|error| {
            tracing::warn!(%error, "the memory's revision could not be read");
            0
        })
    }

    fn read(&self, key: &str) -> Revised {
        self.db.read(|conn| book(conn, key)).unwrap_or_else(|error| {
            tracing::warn!(%error, "the memory could not be read; going on from nothing");
            Revised::default()
        })
    }

    fn update(&self, key: &str, change: &mut (dyn FnMut(&mut MemoryBook) + Send)) -> std::io::Result<Revised> {
        let changed = self.db.write(|conn| {
            alone(conn, "memory_books")?;
            let before = stored_book(conn, key)?;
            let mut after = before.kept.book.clone();
            change(&mut after);
            let revision = keep(conn, key, &before, &after)?;
            Ok(Revised { revision, book: after })
        });
        Ok(changed?)
    }

    fn undo_point(&self, key: &str) -> Option<MemoryBook> {
        let point = self.db.read(|conn| {
            let kept: Option<String> =
                conn.one("SELECT book FROM memory_undo_points WHERE project = ?1", args![key])?;
            kept.map(|text| from_json(0, &text)).transpose()
        });
        point.unwrap_or_else(|error| {
            tracing::warn!(%error, "the memory's undo point could not be read");
            None
        })
    }

    fn set_undo_point(&self, key: &str, book: Option<&MemoryBook>) -> std::io::Result<()> {
        if book.is_none() && self.undo_point(key).is_none() {
            return Ok(());
        }
        let set = self.db.write(|conn| match book {
            Some(book) => conn.execute(
                "INSERT INTO memory_undo_points (project, book) VALUES (?1, ?2) \
                 ON CONFLICT (project) DO UPDATE SET book = excluded.book",
                args![key, json(book)?],
            ),
            None => conn.execute("DELETE FROM memory_undo_points WHERE project = ?1", args![key]),
        });
        Ok(set.map(drop)?)
    }
}

#[derive(Debug, Default)]
pub(super) struct Stored {
    kept: Revised,
    positions: HashMap<MemoryId, i64>,
}

#[derive(Debug, PartialEq)]
struct Entry<'a> {
    position: i64,
    forgotten_at: Option<i64>,
    memory: &'a Memory,
}

fn listed(book: &MemoryBook) -> impl Iterator<Item = (Option<i64>, &Memory)> {
    let known = book.memories.iter().map(|memory| (None, memory));
    known.chain(book.forgotten.iter().map(|forgotten| (Some(forgotten.at), &forgotten.memory)))
}

fn as_stored(stored: &Stored) -> Vec<Entry<'_>> {
    let position = |memory: &Memory| stored.positions.get(&memory.id).copied().unwrap_or(0);
    listed(&stored.kept.book)
        .map(|(forgotten_at, memory)| Entry { position: position(memory), forgotten_at, memory })
        .collect()
}

fn in_their_order<'a>(book: &'a MemoryBook, positions: &HashMap<MemoryId, i64>) -> Vec<Entry<'a>> {
    let mut last: [Option<i64>; 2] = [None, None];
    let mut after_the_last = |forgotten: bool, memory: &Memory| {
        let had = positions.get(&memory.id).copied().unwrap_or(0);
        let position = last[usize::from(forgotten)].map_or(had, |last| had.max(last + 1));
        last[usize::from(forgotten)] = Some(position);
        position
    };
    listed(book)
        .map(|(forgotten_at, memory)| Entry {
            position: after_the_last(forgotten_at.is_some(), memory),
            forgotten_at,
            memory,
        })
        .collect()
}

pub(super) fn is_empty(conn: &mut dyn Conn) -> database::Result<bool> {
    Ok(conn.one("SELECT NOT EXISTS (SELECT 1 FROM memory_books)", &[])?.unwrap_or(true))
}

fn revision(conn: &mut dyn Conn, key: &str) -> database::Result<u64> {
    let revision: Option<i64> = conn.one("SELECT revision FROM memory_books WHERE project = ?1", args![key])?;
    Ok(revision.map_or(0, crate::db::codec::counted))
}

fn book(conn: &mut dyn Conn, key: &str) -> database::Result<Revised> {
    stored_book(conn, key).map(|stored| stored.kept)
}

fn head(row: &Row) -> database::Result<Revised> {
    let tally = |at: usize| -> database::Result<Tally> {
        Ok(Tally { runs: small(row.get(at)?), corrections: small(row.get(at + 1)?), cost_usd: row.get(at + 2)? })
    };
    let scorecard = Scorecard { with: tally(2)?, without: tally(5)?, briefed: small(row.get(8)?) };
    let book = MemoryBook { next: small(row.get(0)?), scorecard, ..MemoryBook::default() };
    Ok(Revised { revision: crate::db::codec::counted(row.get(1)?), book })
}

fn stored_book(conn: &mut dyn Conn, key: &str) -> database::Result<Stored> {
    let Some(found) = conn.row(BOOK, args![key])? else { return Ok(Stored::default()) };
    let mut stored = Stored { kept: head(&found)?, positions: HashMap::new() };
    for row in conn.query(MEMORIES, args![key])? {
        let (forgotten_at, memory, position) = (row.get::<Option<i64>>(1)?, memory(&row)?, row.get(13)?);
        stored.positions.insert(memory.id, position);
        match forgotten_at {
            None => stored.kept.book.memories.push(memory),
            Some(at) => stored.kept.book.forgotten.push(Forgotten { memory, at }),
        }
    }
    Ok(stored)
}

fn memory(row: &Row) -> database::Result<Memory> {
    Ok(Memory {
        id: MemoryId(small(row.get(0)?)),
        kind: named(2, row.get(2)?)?,
        author: named(3, row.get(3)?)?,
        text: row.get(4)?,
        tags: from_json(5, &row.get::<String>(5)?)?,
        touches: from_json(6, &row.get::<String>(6)?)?,
        history: from_json(7, &row.get::<String>(7)?)?,
        created: row.get(8)?,
        origin: row.get(9)?,
        uses: small(row.get(10)?),
        last_used: row.get(11)?,
        credit: row.get(12)?,
    })
}

pub(super) fn keep(conn: &mut dyn Conn, key: &str, before: &Stored, after: &MemoryBook) -> database::Result<u64> {
    if before.kept.book == *after {
        return Ok(before.kept.revision);
    }
    let revision = before.kept.revision + 1;
    let Scorecard { with, without, briefed } = after.scorecard;
    conn.execute(
        KEEP_BOOK,
        args![
            key,
            after.next,
            stored(revision),
            with.runs,
            with.corrections,
            with.cost_usd,
            without.runs,
            without.corrections,
            without.cost_usd,
            briefed
        ],
    )?;
    let earlier = as_stored(before);
    let had: HashMap<MemoryId, &Entry<'_>> = earlier.iter().map(|entry| (entry.memory.id, entry)).collect();
    let wanted = in_their_order(after, &before.positions);
    let held: HashSet<MemoryId> = wanted.iter().map(|entry| entry.memory.id).collect();
    for entry in wanted.iter().filter(|entry| had.get(&entry.memory.id).copied() != Some(*entry)) {
        let Memory { id, kind, text, tags, created, origin, author, history, uses, last_used, touches, credit } =
            entry.memory;
        conn.execute(
            KEEP_MEMORY,
            args![
                key,
                id.0,
                entry.position,
                entry.forgotten_at,
                name(kind)?,
                name(author)?,
                text,
                json(tags)?,
                json(touches)?,
                json(history)?,
                created,
                origin,
                uses,
                last_used,
                credit
            ],
        )?;
    }
    for gone in earlier.iter().filter(|entry| !held.contains(&entry.memory.id)) {
        conn.execute("DELETE FROM memories WHERE project = ?1 AND id = ?2", args![key, gone.memory.id.0])?;
    }
    Ok(revision)
}
