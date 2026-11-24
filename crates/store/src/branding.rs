use domain::Language;
use serde_json::{Map, Value};

use crate::{Result, Store};

impl Store {
    pub fn branding(&self) -> Result<Map<String, Value>> {
        let connection = self.locked();
        let mut statement = connection.prepare("SELECT key, value FROM branding ORDER BY key")?;
        let rows = statement
            .query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        rows.into_iter().map(|(key, value)| Ok((key, serde_json::from_str(&value)?))).collect()
    }

    pub fn output_style(&self) -> Option<String> {
        self.branding().ok()?.get("output_style")?.as_str().map(str::to_owned)
    }

    pub fn language(&self) -> Result<Language> {
        let kept = self.branding()?;
        Ok(kept.get("language").and_then(Value::as_str).and_then(Language::named).unwrap_or_default())
    }

    pub fn set_branding(&self, key: &str, value: &Value) -> Result<()> {
        self.locked().execute(
            "INSERT INTO branding (key, value) VALUES (?1, ?2)
             ON CONFLICT (key) DO UPDATE SET value = excluded.value",
            [key, &serde_json::to_string(value)?],
        )?;
        Ok(())
    }
}
