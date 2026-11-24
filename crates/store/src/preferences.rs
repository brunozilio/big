use rusqlite::OptionalExtension;
use serde_json::Value;

use crate::{Result, Store};

impl Store {
    pub fn preference(&self, key: &str) -> Result<Option<Value>> {
        let connection = self.locked();
        let kept: Option<String> =
            connection.query_row("SELECT value FROM preferences WHERE key = ?1", [key], |row| row.get(0)).optional()?;
        kept.map(|text| Ok(serde_json::from_str(&text)?)).transpose()
    }

    pub fn set_preference(&self, key: &str, value: &Value) -> Result<()> {
        self.locked().execute(
            "INSERT INTO preferences (key, value) VALUES (?1, ?2)
             ON CONFLICT (key) DO UPDATE SET value = excluded.value",
            [key, &serde_json::to_string(value)?],
        )?;
        Ok(())
    }
}
