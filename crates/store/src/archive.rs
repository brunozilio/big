use std::collections::HashMap;

use crate::{Result, Store};

impl Store {
    pub fn archive_item(&self, item: &str, at: i64) -> Result<()> {
        self.locked().execute(
            "INSERT INTO archived_items (item, at) VALUES (?1, ?2) ON CONFLICT (item) DO NOTHING",
            rusqlite::params![item, at],
        )?;
        Ok(())
    }

    pub fn unarchive_item(&self, item: &str) -> Result<()> {
        self.locked().execute("DELETE FROM archived_items WHERE item = ?1", [item])?;
        Ok(())
    }

    pub fn archived_items(&self) -> Result<HashMap<String, i64>> {
        let connection = self.locked();
        let mut statement = connection.prepare("SELECT item, at FROM archived_items")?;
        let rows = statement.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }
}

#[cfg(test)]
mod tests {
    use crate::Store;

    #[test]
    fn an_item_is_archived_once_and_can_come_back() {
        let dir = tempfile::tempdir().expect("dir");
        let store = Store::open(&dir.path().join("app.db"), 0).expect("store");
        store.archive_item("i1", 10).expect("archive");
        store.archive_item("i1", 20).expect("again");
        assert_eq!(store.archived_items().expect("list").get("i1"), Some(&10), "the first time stays");
        store.unarchive_item("i1").expect("unarchive");
        assert!(store.archived_items().expect("list").is_empty());
    }
}
