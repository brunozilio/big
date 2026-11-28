use std::collections::HashMap;

use database::args;

use crate::{Result, Store};

impl Store {
    pub fn archive_item(&self, item: &str, at: i64) -> Result<()> {
        self.write(|conn| {
            conn.execute(
                "INSERT INTO archived_items (item, at) VALUES (?1, ?2) ON CONFLICT (item) DO NOTHING",
                args![item, at],
            )?;
            Ok(())
        })
    }

    pub fn unarchive_item(&self, item: &str) -> Result<()> {
        self.write(|conn| {
            conn.execute("DELETE FROM archived_items WHERE item = ?1", args![item])?;
            Ok(())
        })
    }

    pub fn archived_items(&self) -> Result<HashMap<String, i64>> {
        self.read(|conn| {
            let rows = conn.query("SELECT item, at FROM archived_items", &[])?;
            Ok(rows.iter().map(|row| Ok((row.get(0)?, row.get(1)?))).collect::<database::Result<_>>()?)
        })
    }
}
