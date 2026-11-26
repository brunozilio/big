use rusqlite::{OptionalExtension, params};

use crate::{Result, Store};

impl Store {
    pub fn connector_memory(&self, connection: &str, name: &str) -> Result<Option<String>> {
        let value = self
            .locked()
            .query_row(
                "SELECT value FROM connector_memory WHERE connection = ?1 AND name = ?2",
                params![connection, name],
                |row| row.get(0),
            )
            .optional()?;
        Ok(value)
    }

    pub fn set_connector_memory(&self, connection: &str, name: &str, value: &str) -> Result<()> {
        self.locked().execute(
            "INSERT INTO connector_memory (connection, name, value) VALUES (?1, ?2, ?3)
             ON CONFLICT (connection, name) DO UPDATE SET value = excluded.value",
            params![connection, name, value],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::Store;

    #[test]
    fn a_connector_memory_is_kept_per_connection_and_name() {
        let store = Store::in_memory(0).expect("store");
        assert_eq!(store.connector_memory("a", "ledger").expect("read"), None);
        store.set_connector_memory("a", "ledger", "[1]").expect("write");
        store.set_connector_memory("a", "ledger", "[2]").expect("overwrite");
        store.set_connector_memory("b", "ledger", "[9]").expect("other connection");
        assert_eq!(store.connector_memory("a", "ledger").expect("read"), Some("[2]".to_owned()));
        assert_eq!(store.connector_memory("b", "ledger").expect("read"), Some("[9]".to_owned()));
        assert_eq!(store.connector_memory("a", "other").expect("read"), None);
    }
}
