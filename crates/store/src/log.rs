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
        match stored.map(|text| crate::legacy::item(&text)) {
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
            match crate::legacy::item(&text) {
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
        rows.into_iter()
            .filter_map(|(seq, at, text)| match crate::legacy::event(&text) {
                Ok(Some(event)) => Some(unsigned(seq).map(|seq| (seq, at, event))),
                Ok(None) => None,
                Err(error) => Some(Err(error.into())),
            })
            .collect()
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
    match stored(conn, item)?.map(|text| crate::legacy::item(&text)) {
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
    let mut state = events.iter().map(|(_, event)| event).fold(base.unwrap_or_default(), reduce);
    let last: i64 =
        conn.one("SELECT COALESCE(MAX(seq), 0) FROM work_events WHERE item = ?1", args![item.as_str()])?.unwrap_or(0);
    state.version = state.version.max(unsigned(last)?);
    Ok(Some(state))
}

fn latest_snapshot(conn: &mut dyn Conn, item: &ItemId) -> Result<Option<WorkItem>> {
    let rows =
        conn.query("SELECT seq, state FROM snapshots WHERE item = ?1 ORDER BY seq DESC", args![item.as_str()])?;
    for row in rows {
        let seq = unsigned(row.get(0)?)?;
        let decoded = crate::legacy::item(&row.get::<String>(1)?).ok();
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
    rows.iter()
        .map(|row| Ok((row.get::<i64>(0)?, row.get::<String>(1)?)))
        .collect::<database::Result<Vec<_>>>()?
        .into_iter()
        .filter_map(|(seq, text)| match crate::legacy::event(&text) {
            Ok(Some(event)) => Some(unsigned(seq).map(|seq| (seq, event))),
            Ok(None) => None,
            Err(error) => Some(Err(error.into())),
        })
        .collect()
}

pub(crate) fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| i64::try_from(elapsed.as_millis()).unwrap_or(i64::MAX))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn an_item_kept_before_bindings_loads_its_lane_and_chosen_base_as_bindings() {
        let store = Store::in_memory(0).expect("store");
        let id = ItemId::from("old-2");
        let created =
            WorkflowEvent::ItemCreated { id: id.clone(), workflow: "flow".into(), parent: None, input: json!({}) };
        store.append(&id, &[created]).expect("created");
        let lane = json!({ "type": "branched", "data": { "name": "big/x" } });
        let base = json!({ "type": "base_chosen", "data": { "repository": "web", "base": "main" } });
        store
            .write(|conn| {
                for (seq, event) in [(2, &lane), (3, &base)] {
                    conn.execute(
                        "INSERT INTO work_events (item, seq, event, at) VALUES (?1, ?2, ?3, 0)",
                        args![id.as_str(), seq, event.to_string()],
                    )?;
                }
                let state: String =
                    conn.one("SELECT state FROM items WHERE id = ?1", args![id.as_str()])?.unwrap_or_default();
                let mut state: serde_json::Value = serde_json::from_str(&state)?;
                state["branch"] = json!("big/x");
                state["version"] = json!(3);
                conn.execute("UPDATE items SET state = ?1 WHERE id = ?2", args![state.to_string(), id.as_str()])?;
                Ok(())
            })
            .expect("an old item");
        let text = |item: &WorkItem, connection: &str, kind: &str| {
            item.bound(connection, kind).and_then(domain::workflow::Binding::text).map(str::to_owned)
        };
        let kept = store.item(&id).expect("item").expect("kept");
        assert_eq!(text(&kept, "", "branch").as_deref(), Some("big/x"), "the old projection keeps its lane");
        let rebuilt = store.load(&id).expect("rebuilt").expect("events");
        assert_eq!(text(&rebuilt, "", "branch").as_deref(), Some("big/x"));
        assert_eq!(text(&rebuilt, "web", "base").as_deref(), Some("main"));
        assert_eq!(store.events(&id, 0).expect("events").len(), 3);
        assert!(store.append(&id, &[WorkflowEvent::Named { title: "novo".into() }]).is_ok());
    }

    #[test]
    fn an_item_kept_with_a_retired_heads_up_still_loads() {
        let store = Store::in_memory(0).expect("store");
        let id = ItemId::from("old-1");
        let created =
            WorkflowEvent::ItemCreated { id: id.clone(), workflow: "flow".into(), parent: None, input: json!({}) };
        store.append(&id, &[created]).expect("created");
        let retired = json!({ "type": "heads_up", "data": {
            "step": "develop", "note": { "title": "Migração", "explain": "Altera tabelas.", "urgent": true }
        } });
        store
            .write(|conn| {
                conn.execute(
                    "INSERT INTO work_events (item, seq, event, at) VALUES (?1, 2, ?2, 0)",
                    args![id.as_str(), retired.to_string()],
                )?;
                let state: String =
                    conn.one("SELECT state FROM items WHERE id = ?1", args![id.as_str()])?.unwrap_or_default();
                let mut state: serde_json::Value = serde_json::from_str(&state)?;
                state["heads_up"] = json!({ "develop": { "title": "Migração", "explain": "x", "urgent": true } });
                state["version"] = json!(2);
                conn.execute("UPDATE items SET state = ?1 WHERE id = ?2", args![state.to_string(), id.as_str()])?;
                Ok(())
            })
            .expect("an old event and state");
        assert_eq!(store.events(&id, 0).expect("events").len(), 1, "the retired event is skipped");
        assert_eq!(store.timed_events(&id, 0).expect("timed").len(), 1);
        assert_eq!(store.item(&id).expect("item").map(|item| item.workflow), Some("flow".to_owned()));
        assert_eq!(store.load(&id).expect("rebuilt").map(|item| item.version), Some(2), "the old event still counts");
        let named = WorkflowEvent::Named { title: "novo".into() };
        assert!(store.append(&id, &[named]).is_ok(), "the item keeps going after the old event");
    }
}
