use std::collections::HashSet;
use std::time::Duration;

use rusqlite::{OptionalExtension, params};

use crate::log::now;
use crate::{Result, Store};

impl Store {
    pub fn mark_seen(&self, connection: &str, event: &str) -> Result<bool> {
        let inserted = self.locked().execute(
            "INSERT OR IGNORE INTO seen_events (connection, id, at) VALUES (?1, ?2, ?3)",
            params![connection, event, now()],
        )?;
        Ok(inserted > 0)
    }

    pub fn was_seen(&self, connection: &str, event: &str) -> Result<bool> {
        let renewed = self.locked().execute(
            "UPDATE seen_events SET at = ?3 WHERE connection = ?1 AND id = ?2",
            params![connection, event, now()],
        )?;
        Ok(renewed > 0)
    }

    pub fn seen_among(&self, connection: &str, events: &[String]) -> Result<HashSet<String>> {
        self.write(|transaction| {
            let at = now();
            let mut renew =
                transaction.prepare("UPDATE seen_events SET at = ?3 WHERE connection = ?1 AND id = ?2")?;
            let mut seen = HashSet::new();
            for event in events {
                if renew.execute(params![connection, event, at])? > 0 {
                    seen.insert(event.clone());
                }
            }
            Ok(seen)
        })
    }

    pub fn mark_all_seen(&self, connection: &str, events: &[String]) -> Result<usize> {
        if events.is_empty() {
            return Ok(0);
        }
        self.write(|transaction| {
            let at = now();
            let mut mark = transaction
                .prepare("INSERT OR IGNORE INTO seen_events (connection, id, at) VALUES (?1, ?2, ?3)")?;
            let mut inserted = 0;
            for event in events {
                inserted += mark.execute(params![connection, event, at])?;
            }
            Ok(inserted)
        })
    }

    pub fn prune_seen(&self, retention: Duration) -> Result<usize> {
        let span = i64::try_from(retention.as_millis()).unwrap_or(i64::MAX);
        let cutoff = now().saturating_sub(span);
        let pruned = self.locked().execute(
            "DELETE FROM seen_events WHERE at < ?1 AND at < COALESCE(
                (SELECT moved FROM event_cursors WHERE event_cursors.connection = seen_events.connection), ?1)",
            [cutoff],
        )?;
        Ok(pruned)
    }

    pub fn cursor(&self, connection: &str) -> Result<Option<String>> {
        let cursor = self
            .locked()
            .query_row("SELECT cursor FROM event_cursors WHERE connection = ?1", [connection], |row| row.get(0))
            .optional()?;
        Ok(cursor)
    }

    pub fn set_cursor(&self, connection: &str, cursor: &str) -> Result<()> {
        self.locked().execute(
            "INSERT INTO event_cursors (connection, cursor, moved) VALUES (?1, ?2, ?3)
             ON CONFLICT (connection) DO UPDATE SET cursor = excluded.cursor, moved = excluded.moved",
            params![connection, cursor, now()],
        )?;
        Ok(())
    }
}
