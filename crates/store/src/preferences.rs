use database::args;
use serde_json::Value;

use crate::{Result, Store};

impl Store {
    pub fn preference(&self, key: &str) -> Result<Option<Value>> {
        let kept: Option<String> =
            self.read(|conn| Ok(conn.one("SELECT value FROM preferences WHERE key = ?1", args![key])?))?;
        kept.map(|text| Ok(serde_json::from_str(&text)?)).transpose()
    }

    pub fn set_preference(&self, key: &str, value: &Value) -> Result<()> {
        let text = serde_json::to_string(value)?;
        self.write(|conn| {
            conn.execute(
                "INSERT INTO preferences (key, value) VALUES (?1, ?2)
                 ON CONFLICT (key) DO UPDATE SET value = excluded.value",
                args![key, text],
            )?;
            Ok(())
        })
    }
}
