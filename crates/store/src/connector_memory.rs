use database::args;

use crate::{Result, Store};

impl Store {
    pub fn connector_memory(&self, connection: &str, name: &str) -> Result<Option<String>> {
        self.read(|conn| {
            Ok(conn.one(
                "SELECT value FROM connector_memory WHERE connection = ?1 AND name = ?2",
                args![connection, name],
            )?)
        })
    }

    pub fn set_connector_memory(&self, connection: &str, name: &str, value: &str) -> Result<()> {
        self.write(|conn| {
            conn.execute(
                "INSERT INTO connector_memory (connection, name, value) VALUES (?1, ?2, ?3)
                 ON CONFLICT (connection, name) DO UPDATE SET value = excluded.value",
                args![connection, name, value],
            )?;
            Ok(())
        })
    }
}
