use database::args;
use domain::Language;
use serde_json::{Map, Value};

use crate::{Result, Store};

const PERSONAL: &str = "user.";

fn personal_prefix(owner: &str) -> Option<String> {
    (owner != domain::principal::LOCAL_USER).then(|| format!("{PERSONAL}{owner}."))
}

impl Store {
    fn branding_rows(&self) -> Result<Vec<(String, Value)>> {
        let rows: Vec<(String, String)> = self.read(|conn| {
            let rows = conn.query("SELECT key, value FROM branding ORDER BY key", &[])?;
            Ok(rows.iter().map(|row| Ok((row.get(0)?, row.get(1)?))).collect::<database::Result<_>>()?)
        })?;
        rows.into_iter().map(|(key, value)| Ok((key, serde_json::from_str(&value)?))).collect()
    }

    pub fn branding(&self) -> Result<Map<String, Value>> {
        Ok(self.branding_rows()?.into_iter().filter(|(key, _)| !key.starts_with(PERSONAL)).collect())
    }

    pub fn branding_for(&self, owner: &str) -> Result<Map<String, Value>> {
        let rows = self.branding_rows()?;
        let mut kept: Map<String, Value> = rows.iter().filter(|(key, _)| !key.starts_with(PERSONAL)).cloned().collect();
        if let Some(prefix) = personal_prefix(owner) {
            for (key, value) in rows {
                if let Some(own) = key.strip_prefix(&prefix) {
                    kept.insert(own.to_owned(), value);
                }
            }
        }
        Ok(kept)
    }

    pub fn set_branding_for(&self, owner: &str, key: &str, value: &Value) -> Result<()> {
        match personal_prefix(owner) {
            Some(prefix) => self.set_branding(&format!("{prefix}{key}"), value),
            None => self.set_branding(key, value),
        }
    }

    pub fn language_for(&self, owner: &str) -> Result<Language> {
        let kept = self.branding_for(owner)?;
        Ok(kept.get("language").and_then(Value::as_str).and_then(Language::named).unwrap_or_default())
    }

    pub fn output_style_for(&self, owner: &str) -> Option<String> {
        self.branding_for(owner).ok()?.get("output_style")?.as_str().map(str::to_owned)
    }

    pub fn output_style(&self) -> Option<String> {
        self.branding().ok()?.get("output_style")?.as_str().map(str::to_owned)
    }

    pub fn language(&self) -> Result<Language> {
        let kept = self.branding()?;
        Ok(kept.get("language").and_then(Value::as_str).and_then(Language::named).unwrap_or_default())
    }

    pub fn set_branding(&self, key: &str, value: &Value) -> Result<()> {
        let text = serde_json::to_string(value)?;
        self.write(|conn| {
            conn.execute(
                "INSERT INTO branding (key, value) VALUES (?1, ?2)
                 ON CONFLICT (key) DO UPDATE SET value = excluded.value",
                args![key, text],
            )?;
            Ok(())
        })
    }
}
