use rusqlite::{OptionalExtension, params};
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

impl Store {
    pub fn add_browser_device(&self, token: &str, device: &BrowserDevice) -> Result<()> {
        self.write(|transaction| {
            transaction.execute(
                "DELETE FROM browser_devices WHERE device = ?1 AND owner = ?2",
                params![device.device, device.owner],
            )?;
            transaction.execute(
                "INSERT INTO browser_devices (token, device, owner, name, created) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![token, device.device, device.owner, device.name, device.created],
            )?;
            Ok(())
        })
    }

    pub fn browser_device(&self, token: &str) -> Result<Option<BrowserDevice>> {
        Ok(self
            .locked()
            .query_row("SELECT device, owner, name, created FROM browser_devices WHERE token = ?1", [token], |row| {
                Ok(BrowserDevice { device: row.get(0)?, owner: row.get(1)?, name: row.get(2)?, created: row.get(3)? })
            })
            .optional()?)
    }

    pub fn browser_devices_of(&self, owner: &str) -> Result<Vec<BrowserDevice>> {
        let held = self.locked();
        let mut query = held.prepare(
            "SELECT device, owner, name, created FROM browser_devices WHERE owner = ?1 ORDER BY created DESC",
        )?;
        let rows = query.query_map([owner], |row| {
            Ok(BrowserDevice { device: row.get(0)?, owner: row.get(1)?, name: row.get(2)?, created: row.get(3)? })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub fn forget_browser_device(&self, owner: &str, device: &str) -> Result<bool> {
        let removed = self
            .locked()
            .execute("DELETE FROM browser_devices WHERE owner = ?1 AND device = ?2", params![owner, device])?;
        Ok(removed > 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn device(id: &str, owner: &str) -> BrowserDevice {
        BrowserDevice { device: id.into(), owner: owner.into(), name: format!("Chrome {id}"), created: 1 }
    }

    #[test]
    fn a_paired_browser_is_found_by_its_token_and_pairing_again_replaces_the_old_token() {
        let store = Store::in_memory(0).unwrap_or_else(|_| unreachable!());
        store.add_browser_device("t1", &device("a", "ana")).unwrap_or_default();
        assert_eq!(store.browser_device("t1").ok().flatten(), Some(device("a", "ana")));
        store.add_browser_device("t2", &device("a", "ana")).unwrap_or_default();
        assert_eq!(store.browser_device("t1").ok().flatten(), None);
        store.add_browser_device("t3", &device("b", "bia")).unwrap_or_default();
        assert_eq!(store.browser_devices_of("ana").map(|all| all.len()).unwrap_or_default(), 1);
        assert!(!store.forget_browser_device("bia", "a").unwrap_or(true));
        assert!(store.forget_browser_device("ana", "a").unwrap_or_default());
        assert_eq!(store.browser_device("t2").ok().flatten(), None);
    }
}
