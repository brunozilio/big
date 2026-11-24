use std::time::{SystemTime, UNIX_EPOCH};

use domain::workflow::{ItemId, WorkItem, WorkflowEvent, reduce};
use rusqlite::{Connection, OptionalExtension, Transaction, params};

use crate::{Result, Store, signed, unsigned};

impl Store {
    pub fn append(&self, item: &ItemId, events: &[WorkflowEvent]) -> Result<WorkItem> {
        let every = self.snapshot_every;
        self.write(|transaction| {
            let before = current(transaction, item)?.unwrap_or_default();
            let start = before.version;
            let at = now();
            let mut state = before;
            for event in events {
                state = reduce(state, event);
                transaction.execute(
                    "INSERT INTO work_events (item, seq, event, at) VALUES (?1, ?2, ?3, ?4)",
                    params![item.as_str(), signed(state.version)?, serde_json::to_string(event)?, at],
                )?;
            }
            if state.version != start {
                project(transaction, item, &state)?;
            }
            if every > 0 && state.version / every > start / every {
                transaction.execute(
                    "INSERT OR REPLACE INTO snapshots (item, seq, state) VALUES (?1, ?2, ?3)",
                    params![item.as_str(), signed(state.version)?, serde_json::to_string(&state)?],
                )?;
            }
            Ok(state)
        })
    }

    pub fn load(&self, item: &ItemId) -> Result<Option<WorkItem>> {
        rebuilt(&self.locked(), item)
    }

    pub fn item(&self, item: &ItemId) -> Result<Option<WorkItem>> {
        let stored = {
            let connection = self.locked();
            stored(&connection, item)?
        };
        match stored.map(|text| serde_json::from_str::<WorkItem>(&text)) {
            Some(Ok(state)) => Ok(Some(state)),
            Some(Err(_)) => self.heal(item),
            None => Ok(None),
        }
    }

    pub fn items(&self) -> Result<Vec<WorkItem>> {
        self.listed("SELECT id, state FROM items ORDER BY rowid")
    }

    pub fn active_items(&self) -> Result<Vec<WorkItem>> {
        self.listed("SELECT id, state FROM items WHERE ended = 0 ORDER BY rowid")
    }

    fn listed(&self, query: &str) -> Result<Vec<WorkItem>> {
        let rows = {
            let connection = self.locked();
            let mut statement = connection.prepare(query)?;
            statement
                .query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))?
                .collect::<rusqlite::Result<Vec<_>>>()?
        };
        let mut items = Vec::with_capacity(rows.len());
        for (id, text) in rows {
            match serde_json::from_str::<WorkItem>(&text) {
                Ok(state) => items.push(state),
                Err(_) => items.extend(self.heal(&ItemId::from(id))?),
            }
        }
        Ok(items)
    }

    fn heal(&self, item: &ItemId) -> Result<Option<WorkItem>> {
        self.write(|transaction| {
            let state = rebuilt(transaction, item)?;
            if let Some(state) = &state {
                project(transaction, item, state)?;
            }
            Ok(state)
        })
    }

    pub fn events(&self, item: &ItemId, after: u64) -> Result<Vec<(u64, WorkflowEvent)>> {
        events_after(&self.locked(), item, after)
    }

    pub fn timed_events(&self, item: &ItemId, after: u64) -> Result<Vec<(u64, i64, WorkflowEvent)>> {
        let connection = self.locked();
        let mut statement =
            connection.prepare("SELECT seq, at, event FROM work_events WHERE item = ?1 AND seq > ?2 ORDER BY seq")?;
        let rows = statement
            .query_map(params![item.as_str(), signed(after)?], |row| {
                Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?, row.get::<_, String>(2)?))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        rows.into_iter().map(|(seq, at, text)| Ok((unsigned(seq)?, at, serde_json::from_str(&text)?))).collect()
    }

    pub fn snapshot_versions(&self, item: &ItemId) -> Result<Vec<u64>> {
        let connection = self.locked();
        let mut statement = connection.prepare("SELECT seq FROM snapshots WHERE item = ?1 ORDER BY seq")?;
        let versions =
            statement.query_map([item.as_str()], |row| row.get(0))?.collect::<rusqlite::Result<Vec<i64>>>()?;
        versions.into_iter().map(unsigned).collect()
    }
}

fn stored(connection: &Connection, item: &ItemId) -> Result<Option<String>> {
    let text = connection
        .query_row("SELECT state FROM items WHERE id = ?1", [item.as_str()], |row| row.get(0))
        .optional()?;
    Ok(text)
}

fn current(connection: &Connection, item: &ItemId) -> Result<Option<WorkItem>> {
    match stored(connection, item)?.map(|text| serde_json::from_str::<WorkItem>(&text)) {
        Some(Ok(state)) => Ok(Some(state)),
        Some(Err(_)) | None => rebuilt(connection, item),
    }
}

fn rebuilt(connection: &Connection, item: &ItemId) -> Result<Option<WorkItem>> {
    let base = latest_snapshot(connection, item)?;
    let after = base.as_ref().map_or(0, |state| state.version);
    let events = events_after(connection, item, after)?;
    if base.is_none() && events.is_empty() {
        return Ok(None);
    }
    Ok(Some(events.iter().map(|(_, event)| event).fold(base.unwrap_or_default(), reduce)))
}

fn latest_snapshot(connection: &Connection, item: &ItemId) -> Result<Option<WorkItem>> {
    let mut statement = connection.prepare("SELECT seq, state FROM snapshots WHERE item = ?1 ORDER BY seq DESC")?;
    let mut rows = statement.query([item.as_str()])?;
    while let Some(row) = rows.next()? {
        let seq = unsigned(row.get(0)?)?;
        let decoded = serde_json::from_str::<WorkItem>(&row.get::<_, String>(1)?).ok();
        if let Some(state) = decoded.filter(|state| state.version == seq) {
            return Ok(Some(state));
        }
    }
    Ok(None)
}

fn project(transaction: &Transaction<'_>, item: &ItemId, state: &WorkItem) -> Result<()> {
    transaction.execute(
        "INSERT INTO items (id, workflow, parent, version, ended, state) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
         ON CONFLICT (id) DO UPDATE SET workflow = excluded.workflow, parent = excluded.parent,
         version = excluded.version, ended = excluded.ended, state = excluded.state",
        params![
            item.as_str(),
            state.workflow,
            state.parent.as_ref().map(ItemId::as_str),
            signed(state.version)?,
            state.is_ended(),
            serde_json::to_string(state)?,
        ],
    )?;
    Ok(())
}

fn events_after(connection: &Connection, item: &ItemId, after: u64) -> Result<Vec<(u64, WorkflowEvent)>> {
    let mut statement =
        connection.prepare("SELECT seq, event FROM work_events WHERE item = ?1 AND seq > ?2 ORDER BY seq")?;
    let rows = statement
        .query_map(params![item.as_str(), signed(after)?], |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    rows.into_iter().map(|(seq, text)| Ok((unsigned(seq)?, serde_json::from_str(&text)?))).collect()
}

pub(crate) fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| i64::try_from(elapsed.as_millis()).unwrap_or(i64::MAX))
}
