use rusqlite::{OptionalExtension, Row, params};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{Error, Result, Store};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ConnectionRecord {
    pub id: String,
    pub provider: String,
    pub label: String,
    pub account: Option<String>,
    pub capabilities: Vec<String>,
    pub settings: Value,
}

const COLUMNS: &str = "SELECT id, provider, label, account, capabilities, settings FROM connections";

type Raw = (String, String, String, Option<String>, String, String);

impl Store {
    pub fn save_connection(&self, record: &ConnectionRecord) -> Result<()> {
        self.locked().execute(
            "INSERT INTO connections (id, provider, label, account, capabilities, settings)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT (id) DO UPDATE SET provider = excluded.provider, label = excluded.label,
             account = excluded.account, capabilities = excluded.capabilities, settings = excluded.settings",
            params![
                record.id,
                record.provider,
                record.label,
                record.account,
                serde_json::to_string(&record.capabilities)?,
                serde_json::to_string(&record.settings)?,
            ],
        )?;
        Ok(())
    }

    pub fn connection(&self, id: &str) -> Result<Option<ConnectionRecord>> {
        let raw = self.locked().query_row(&format!("{COLUMNS} WHERE id = ?1"), [id], raw).optional()?;
        raw.map(record).transpose()
    }

    pub fn connections(&self) -> Result<Vec<ConnectionRecord>> {
        let connection = self.locked();
        let mut statement = connection.prepare(&format!("{COLUMNS} ORDER BY rowid"))?;
        let rows = statement.query_map([], raw)?.collect::<rusqlite::Result<Vec<_>>>()?;
        rows.into_iter().map(record).collect()
    }

    pub fn delete_connection(&self, id: &str) -> Result<bool> {
        Ok(self.locked().execute("DELETE FROM connections WHERE id = ?1", [id])? > 0)
    }
}

fn raw(row: &Row<'_>) -> rusqlite::Result<Raw> {
    Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?, row.get(5)?))
}

fn record((id, provider, label, account, capabilities, settings): Raw) -> Result<ConnectionRecord> {
    Ok(ConnectionRecord {
        id,
        provider,
        label,
        account,
        capabilities: serde_json::from_str(&capabilities).map_err(Error::from)?,
        settings: serde_json::from_str(&settings)?,
    })
}
