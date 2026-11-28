use database::{Row, args};
use serde::Serialize;

use crate::{Result, Store};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserDevice {
    pub device: String,
    pub owner: String,
    pub name: String,
    pub created: i64,
}

fn device(row: &Row) -> database::Result<BrowserDevice> {
    Ok(BrowserDevice { device: row.get(0)?, owner: row.get(1)?, name: row.get(2)?, created: row.get(3)? })
}

impl Store {
    pub fn add_browser_device(&self, token: &str, device: &BrowserDevice) -> Result<()> {
        self.write(|conn| {
            conn.execute(
                "DELETE FROM browser_devices WHERE device = ?1 AND owner = ?2",
                args![device.device, device.owner],
            )?;
            conn.execute(
                "INSERT INTO browser_devices (token, device, owner, name, created) VALUES (?1, ?2, ?3, ?4, ?5)",
                args![token, device.device, device.owner, device.name, device.created],
            )?;
            Ok(())
        })
    }

    pub fn browser_device(&self, token: &str) -> Result<Option<BrowserDevice>> {
        self.read(|conn| {
            let row =
                conn.row("SELECT device, owner, name, created FROM browser_devices WHERE token = ?1", args![token])?;
            Ok(row.as_ref().map(device).transpose()?)
        })
    }

    pub fn browser_devices_of(&self, owner: &str) -> Result<Vec<BrowserDevice>> {
        self.read(|conn| {
            let rows = conn.query(
                "SELECT device, owner, name, created FROM browser_devices WHERE owner = ?1 ORDER BY created DESC",
                args![owner],
            )?;
            Ok(rows.iter().map(device).collect::<database::Result<_>>()?)
        })
    }

    pub fn forget_browser_device(&self, owner: &str, device: &str) -> Result<bool> {
        let removed = self.write(|conn| {
            Ok(conn.execute("DELETE FROM browser_devices WHERE owner = ?1 AND device = ?2", args![owner, device])?)
        })?;
        Ok(removed > 0)
    }
}
