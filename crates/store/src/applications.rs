use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};

use crate::{Result, Store};

const SEEDED: &str = "applications.seeded";

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrackerChoice {
    pub connection: String,
    pub project: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CodeChoice {
    pub connection: String,
    pub repo: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Repository {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub code: Option<CodeChoice>,
    pub directory: String,
    #[serde(default)]
    pub run: String,
    #[serde(default)]
    pub verify: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Application {
    #[serde(default)]
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub tracker: Option<TrackerChoice>,
    #[serde(default)]
    pub repositories: Vec<Repository>,
}

impl Store {
    pub fn applications(&self) -> Result<Vec<Application>> {
        let connection = self.locked();
        let mut statement = connection.prepare("SELECT data FROM applications ORDER BY position, rowid")?;
        let rows = statement.query_map([], |row| row.get::<_, String>(0))?.collect::<rusqlite::Result<Vec<_>>>()?;
        rows.iter().map(|data| Ok(serde_json::from_str(data)?)).collect()
    }

    pub fn application(&self, id: &str) -> Result<Option<Application>> {
        let found: Option<String> = self
            .locked()
            .query_row("SELECT data FROM applications WHERE id = ?1", [id], |row| row.get(0))
            .optional()?;
        found.map(|data| Ok(serde_json::from_str(&data)?)).transpose()
    }

    pub fn save_application(&self, application: &Application) -> Result<()> {
        let data = serde_json::to_string(application)?;
        self.write(|transaction| {
            let next: i64 =
                transaction
                    .query_row("SELECT COALESCE(MAX(position), -1) + 1 FROM applications", [], |row| row.get(0))?;
            transaction.execute(
                "INSERT INTO applications (id, position, data) VALUES (?1, ?2, ?3)
                 ON CONFLICT (id) DO UPDATE SET data = excluded.data",
                params![application.id, next, data],
            )?;
            Ok(())
        })
    }

    pub fn delete_application(&self, id: &str) -> Result<bool> {
        Ok(self.locked().execute("DELETE FROM applications WHERE id = ?1", [id])? > 0)
    }

    pub fn applications_seeded(&self) -> Result<bool> {
        let found: Option<String> = self
            .locked()
            .query_row("SELECT value FROM preferences WHERE key = ?1", [SEEDED], |row| row.get(0))
            .optional()?;
        Ok(found.is_some())
    }

    pub fn seed_applications(&self, defaults: &[Application]) -> Result<bool> {
        let data: Vec<(String, String)> = defaults
            .iter()
            .map(|application| Ok((application.id.clone(), serde_json::to_string(application)?)))
            .collect::<Result<_>>()?;
        self.write(|transaction| {
            let inserted =
                transaction.execute("INSERT OR IGNORE INTO preferences (key, value) VALUES (?1, 'true')", [SEEDED])?;
            if inserted == 0 {
                return Ok(false);
            }
            for (position, (id, data)) in data.iter().enumerate() {
                let position = i64::try_from(position).unwrap_or(i64::MAX);
                transaction.execute(
                    "INSERT OR IGNORE INTO applications (id, position, data) VALUES (?1, ?2, ?3)",
                    params![id, position, data],
                )?;
            }
            Ok(true)
        })
    }
}
