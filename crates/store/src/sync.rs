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
        let found = self
            .locked()
            .query_row("SELECT 1 FROM seen_events WHERE connection = ?1 AND id = ?2", [connection, event], |_| Ok(()))
            .optional()?;
        Ok(found.is_some())
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
            "INSERT INTO event_cursors (connection, cursor) VALUES (?1, ?2)
             ON CONFLICT (connection) DO UPDATE SET cursor = excluded.cursor",
            [connection, cursor],
        )?;
        Ok(())
    }
}
