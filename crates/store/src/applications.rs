use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};

use crate::{Result, Store};

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
    #[serde(default, deserialize_with = "one_or_many")]
    pub run: Vec<String>,
    #[serde(default)]
    pub verify: Vec<String>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum Commands {
    One(String),
    Many(Vec<String>),
}

fn one_or_many<'de, D: serde::Deserializer<'de>>(deserializer: D) -> std::result::Result<Vec<String>, D::Error> {
    Ok(match Commands::deserialize(deserializer)? {
        Commands::One(line) => Some(line).filter(|line| !line.trim().is_empty()).into_iter().collect(),
        Commands::Many(lines) => lines,
    })
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Application {
    #[serde(default)]
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub tracker: Option<TrackerChoice>,
    #[serde(default)]
    pub tests: Option<TrackerChoice>,
    #[serde(default)]
    pub repositories: Vec<Repository>,
    #[serde(rename = "skipDependabot", default = "on")]
    pub skip_dependabot: bool,
}

fn on() -> bool {
    true
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
}
