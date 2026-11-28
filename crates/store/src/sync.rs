use std::collections::HashSet;
use std::time::Duration;

use database::args;

use crate::log::now;
use crate::{Result, Store};

const MARK: &str = "INSERT INTO seen_events (connection, id, at) VALUES (?1, ?2, ?3)
     ON CONFLICT (connection, id) DO NOTHING";

const RENEW: &str = "UPDATE seen_events SET at = ?3 WHERE connection = ?1 AND id = ?2";

impl Store {
    pub fn mark_seen(&self, connection: &str, event: &str) -> Result<bool> {
        let inserted = self.write(|conn| Ok(conn.execute(MARK, args![connection, event, now()])?))?;
        Ok(inserted > 0)
    }

    pub fn was_seen(&self, connection: &str, event: &str) -> Result<bool> {
        let renewed = self.write(|conn| Ok(conn.execute(RENEW, args![connection, event, now()])?))?;
        Ok(renewed > 0)
    }

    pub fn seen_among(&self, connection: &str, events: &[String]) -> Result<HashSet<String>> {
        self.write(|conn| {
            let at = now();
            let mut seen = HashSet::new();
            for event in events {
                if conn.execute(RENEW, args![connection, event, at])? > 0 {
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
        self.write(|conn| {
            let at = now();
            let mut inserted = 0;
            for event in events {
                inserted += usize::try_from(conn.execute(MARK, args![connection, event, at])?).unwrap_or(0);
            }
            Ok(inserted)
        })
    }

    pub fn prune_seen(&self, retention: Duration) -> Result<usize> {
        let span = i64::try_from(retention.as_millis()).unwrap_or(i64::MAX);
        let cutoff = now().saturating_sub(span);
        let pruned = self.write(|conn| {
            Ok(conn.execute(
                "DELETE FROM seen_events WHERE at < ?1 AND at < COALESCE(
                    (SELECT moved FROM event_cursors WHERE event_cursors.connection = seen_events.connection), ?1)",
                args![cutoff],
            )?)
        })?;
        Ok(usize::try_from(pruned).unwrap_or(usize::MAX))
    }

    pub fn cursor(&self, connection: &str) -> Result<Option<String>> {
        self.read(|conn| Ok(conn.one("SELECT cursor FROM event_cursors WHERE connection = ?1", args![connection])?))
    }

    pub fn set_cursor(&self, connection: &str, cursor: &str) -> Result<()> {
        self.write(|conn| {
            conn.execute(
                "INSERT INTO event_cursors (connection, cursor, moved) VALUES (?1, ?2, ?3)
                 ON CONFLICT (connection) DO UPDATE SET cursor = excluded.cursor, moved = excluded.moved",
                args![connection, cursor, now()],
            )?;
            Ok(())
        })
    }
}
