use database::{Row, args};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::schema::next_seq;
use crate::{Result, Store};

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
        let capabilities = serde_json::to_string(&record.capabilities)?;
        let settings = serde_json::to_string(&record.settings)?;
        self.write(|conn| {
            conn.execute(
                &format!(
                    "INSERT INTO connections (id, provider, label, account, capabilities, settings, seq)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, {})
                     ON CONFLICT (id) DO UPDATE SET provider = excluded.provider, label = excluded.label,
                     account = excluded.account, capabilities = excluded.capabilities, settings = excluded.settings",
                    next_seq("connections")
                ),
                args![record.id, record.provider, record.label, record.account, capabilities, settings],
            )?;
            Ok(())
        })
    }

    pub fn connection(&self, id: &str) -> Result<Option<ConnectionRecord>> {
        let raw = self.read(|conn| {
            let row = conn.row(&format!("{COLUMNS} WHERE id = ?1"), args![id])?;
            Ok(row.as_ref().map(raw).transpose()?)
        })?;
        raw.map(record).transpose()
    }

    pub fn connections(&self) -> Result<Vec<ConnectionRecord>> {
        let rows = self.read(|conn| {
            let rows = conn.query(&format!("{COLUMNS} ORDER BY seq, id"), &[])?;
            Ok(rows.iter().map(raw).collect::<database::Result<Vec<_>>>()?)
        })?;
        rows.into_iter().map(record).collect()
    }

    pub fn delete_connection(&self, id: &str) -> Result<bool> {
        Ok(self.write(|conn| Ok(conn.execute("DELETE FROM connections WHERE id = ?1", args![id])?))? > 0)
    }
}

fn raw(row: &Row) -> database::Result<Raw> {
    Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?, row.get(5)?))
}

fn record((id, provider, label, account, capabilities, settings): Raw) -> Result<ConnectionRecord> {
    Ok(ConnectionRecord {
        id,
        provider,
        label,
        account,
        capabilities: serde_json::from_str(&capabilities)?,
        settings: serde_json::from_str(&settings)?,
    })
}
