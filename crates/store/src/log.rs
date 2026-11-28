use std::time::{SystemTime, UNIX_EPOCH};

use database::{Conn, args};
use domain::workflow::{ItemId, WorkItem, WorkflowEvent, reduce};

use crate::schema::next_seq;
use crate::{Result, Store, signed, unsigned};

impl Store {
    pub fn append(&self, item: &ItemId, events: &[WorkflowEvent]) -> Result<WorkItem> {
        let every = self.snapshot_every;
        self.write(|conn| {
            let before = current(conn, item)?.unwrap_or_default();
            let start = before.version;
            let at = now();
            let mut state = before;
            for event in events {
                state = reduce(state, event);
                conn.execute(
                    "INSERT INTO work_events (item, seq, event, at) VALUES (?1, ?2, ?3, ?4)",
                    args![item.as_str(), signed(state.version)?, serde_json::to_string(event)?, at],
                )?;
            }
            if state.version != start {
                project(conn, item, &state)?;
            }
            if every > 0 && state.version / every > start / every {
                conn.execute(
                    "INSERT INTO snapshots (item, seq, state) VALUES (?1, ?2, ?3)
                     ON CONFLICT (item, seq) DO UPDATE SET state = excluded.state",
                    args![item.as_str(), signed(state.version)?, serde_json::to_string(&state)?],
                )?;
            }
            Ok(state)
        })
    }

    pub fn load(&self, item: &ItemId) -> Result<Option<WorkItem>> {
        self.read(|conn| rebuilt(conn, item))
    }

    pub fn item(&self, item: &ItemId) -> Result<Option<WorkItem>> {
        let stored = self.read(|conn| stored(conn, item))?;
        match stored.map(|text| serde_json::from_str::<WorkItem>(&text)) {
            Some(Ok(state)) => Ok(Some(state)),
            Some(Err(_)) => self.heal(item),
            None => Ok(None),
        }
    }

    pub fn items(&self) -> Result<Vec<WorkItem>> {
        self.listed("SELECT id, state FROM items ORDER BY seq, id")
    }

    pub fn active_items(&self) -> Result<Vec<WorkItem>> {
        self.listed("SELECT id, state FROM items WHERE ended = 0 ORDER BY seq, id")
    }

    fn listed(&self, query: &str) -> Result<Vec<WorkItem>> {
        let rows: Vec<(String, String)> = self.read(|conn| {
            let rows = conn.query(query, &[])?;
            Ok(rows.iter().map(|row| Ok((row.get(0)?, row.get(1)?))).collect::<database::Result<_>>()?)
        })?;
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
        self.write(|conn| {
            let state = rebuilt(conn, item)?;
            if let Some(state) = &state {
                project(conn, item, state)?;
            }
            Ok(state)
        })
    }

    pub fn events(&self, item: &ItemId, after: u64) -> Result<Vec<(u64, WorkflowEvent)>> {
        self.read(|conn| events_after(conn, item, after))
    }

    pub fn timed_events(&self, item: &ItemId, after: u64) -> Result<Vec<(u64, i64, WorkflowEvent)>> {
        let rows = self.read(|conn| {
            let rows = conn.query(
                "SELECT seq, at, event FROM work_events WHERE item = ?1 AND seq > ?2 ORDER BY seq",
                args![item.as_str(), signed(after)?],
            )?;
            Ok(rows
                .iter()
                .map(|row| Ok((row.get::<i64>(0)?, row.get::<i64>(1)?, row.get::<String>(2)?)))
                .collect::<database::Result<Vec<_>>>()?)
        })?;
        rows.into_iter().map(|(seq, at, text)| Ok((unsigned(seq)?, at, serde_json::from_str(&text)?))).collect()
    }

    pub fn snapshot_versions(&self, item: &ItemId) -> Result<Vec<u64>> {
        let versions: Vec<i64> = self.read(|conn| {
            Ok(conn.all("SELECT seq FROM snapshots WHERE item = ?1 ORDER BY seq", args![item.as_str()])?)
        })?;
        versions.into_iter().map(unsigned).collect()
    }
}

fn stored(conn: &mut dyn Conn, item: &ItemId) -> Result<Option<String>> {
    Ok(conn.one("SELECT state FROM items WHERE id = ?1", args![item.as_str()])?)
}

fn current(conn: &mut dyn Conn, item: &ItemId) -> Result<Option<WorkItem>> {
    match stored(conn, item)?.map(|text| serde_json::from_str::<WorkItem>(&text)) {
        Some(Ok(state)) => Ok(Some(state)),
        Some(Err(_)) | None => rebuilt(conn, item),
    }
}

fn rebuilt(conn: &mut dyn Conn, item: &ItemId) -> Result<Option<WorkItem>> {
    let base = latest_snapshot(conn, item)?;
    let after = base.as_ref().map_or(0, |state| state.version);
    let events = events_after(conn, item, after)?;
    if base.is_none() && events.is_empty() {
        return Ok(None);
    }
    Ok(Some(events.iter().map(|(_, event)| event).fold(base.unwrap_or_default(), reduce)))
}

fn latest_snapshot(conn: &mut dyn Conn, item: &ItemId) -> Result<Option<WorkItem>> {
    let rows =
        conn.query("SELECT seq, state FROM snapshots WHERE item = ?1 ORDER BY seq DESC", args![item.as_str()])?;
    for row in rows {
        let seq = unsigned(row.get(0)?)?;
        let decoded = serde_json::from_str::<WorkItem>(&row.get::<String>(1)?).ok();
        if let Some(state) = decoded.filter(|state| state.version == seq) {
            return Ok(Some(state));
        }
    }
    Ok(None)
}

fn project(conn: &mut dyn Conn, item: &ItemId, state: &WorkItem) -> Result<()> {
    conn.execute(
        &format!(
            "INSERT INTO items (id, workflow, parent, version, ended, state, seq) VALUES (?1, ?2, ?3, ?4, ?5, ?6, {})
             ON CONFLICT (id) DO UPDATE SET workflow = excluded.workflow, parent = excluded.parent,
             version = excluded.version, ended = excluded.ended, state = excluded.state",
            next_seq("items")
        ),
        args![
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

fn events_after(conn: &mut dyn Conn, item: &ItemId, after: u64) -> Result<Vec<(u64, WorkflowEvent)>> {
    let rows = conn.query(
        "SELECT seq, event FROM work_events WHERE item = ?1 AND seq > ?2 ORDER BY seq",
        args![item.as_str(), signed(after)?],
    )?;
    rows.iter().map(|row| Ok((unsigned(row.get(0)?)?, serde_json::from_str(&row.get::<String>(1)?)?))).collect()
}

pub(crate) fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| i64::try_from(elapsed.as_millis()).unwrap_or(i64::MAX))
}
