use rusqlite::OptionalExtension;

use crate::{Result, Store};

const CODE: &str = "workflow-code:";
const PIN: &str = "workflow-pin:";

impl Store {
    pub fn pin_workflow(&self, item: &str, version: &str, code: &str) -> Result<()> {
        self.write(|transaction| {
            transaction.execute(
                "INSERT OR IGNORE INTO preferences (key, value) VALUES (?1, ?2)",
                [&format!("{CODE}{version}"), code],
            )?;
            transaction.execute(
                "INSERT INTO preferences (key, value) VALUES (?1, ?2)
                 ON CONFLICT (key) DO UPDATE SET value = excluded.value",
                [&format!("{PIN}{item}"), version],
            )?;
            Ok(())
        })
    }

    pub fn pinned_workflow(&self, item: &str) -> Result<Option<(String, String)>> {
        let connection = self.locked();
        let read = |key: String| {
            connection.query_row("SELECT value FROM preferences WHERE key = ?1", [key], |row| row.get::<_, String>(0)).optional()
        };
        let Some(version) = read(format!("{PIN}{item}"))? else { return Ok(None) };
        Ok(read(format!("{CODE}{version}"))?.map(|code| (version, code)))
    }
}
