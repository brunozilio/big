use std::collections::BTreeMap;

use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::{Result, Store};

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Binding {
    pub connection: String,
    pub value: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub label: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub more: Vec<Choice>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub fields: Fields,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub profiles: Vec<Fields>,
}

pub type Fields = BTreeMap<String, Choice>;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Choice {
    pub value: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub label: String,
}

impl Binding {
    pub fn new(connection: &str, value: &str) -> Binding {
        Binding { connection: connection.to_owned(), value: value.to_owned(), ..Binding::default() }
    }

    pub fn is_set(&self) -> bool {
        !self.connection.trim().is_empty()
            && (!self.value.trim().is_empty() || !self.fields.is_empty() || !self.profiles.is_empty())
    }

    pub fn values(&self) -> impl Iterator<Item = &str> {
        std::iter::once(self.value.as_str())
            .chain(self.more.iter().map(|choice| choice.value.as_str()))
            .filter(|value| !value.is_empty())
    }

    pub fn holds(&self, value: &str) -> bool {
        !value.is_empty() && self.values().any(|known| known.eq_ignore_ascii_case(value))
    }
}

fn holders(application: &Application) -> impl Iterator<Item = &Binding> {
    application
        .bindings
        .values()
        .chain(application.repositories.iter().flat_map(|repository| repository.bindings.values()))
        .filter(|binding| binding.is_set())
}

pub fn watched(applications: &[Application], connection: &str) -> Vec<String> {
    let mut found: Vec<String> = Vec::new();
    for binding in applications.iter().flat_map(holders).filter(|binding| binding.connection == connection) {
        for value in binding.values() {
            if !found.iter().any(|known| known == value) {
                found.push(value.to_owned());
            }
        }
    }
    found
}

pub type Bindings = BTreeMap<String, Binding>;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Repository {
    #[serde(default)]
    pub id: String,
    pub directory: String,
    #[serde(default, deserialize_with = "one_or_many")]
    pub run: Vec<String>,
    #[serde(default)]
    pub verify: Vec<String>,
    #[serde(default)]
    pub e2e: Vec<String>,
    #[serde(rename = "e2eDirectory", default)]
    pub e2e_directory: String,
    #[serde(default)]
    pub bindings: Bindings,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub flow: String,
}

impl Repository {
    pub fn bound(&self, capability: &str) -> Option<&Binding> {
        self.bindings.get(capability).filter(|binding| binding.is_set())
    }
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
    pub bindings: Bindings,
    #[serde(default)]
    pub repositories: Vec<Repository>,
    #[serde(default)]
    pub env: Environment,
}

pub type Environment = BTreeMap<String, String>;

impl Application {
    pub fn bound(&self, capability: &str) -> Option<&Binding> {
        self.bindings.get(capability).filter(|binding| binding.is_set())
    }
}

const LEGACY: [(&str, &str); 4] =
    [("tracker", "project"), ("tests", "project"), ("security", "project"), ("code", "repo")];

fn legacy_binding(chosen: &Value, capability: &str) -> Option<Value> {
    let connection = chosen["connection"].as_str().filter(|connection| !connection.is_empty())?;
    let value = if capability == "cloud" {
        let account = chosen["account"].as_str().filter(|account| !account.is_empty())?;
        let role = chosen["role"].as_str().unwrap_or_default();
        if role.is_empty() { account.to_owned() } else { format!("{account}/{role}") }
    } else {
        let field = LEGACY.iter().find(|(name, _)| *name == capability).map_or("project", |(_, field)| *field);
        chosen[field].as_str().filter(|value| !value.is_empty())?.to_owned()
    };
    Some(json!({ "connection": connection, "value": value }))
}

fn lift(holder: &mut Value, capabilities: &[&str]) -> Vec<(String, Value)> {
    let mut lifted = Vec::new();
    for capability in capabilities {
        let Some(chosen) = holder.as_object_mut().and_then(|fields| fields.remove(*capability)) else { continue };
        if let Some(binding) = legacy_binding(&chosen, capability) {
            lifted.push(((*capability).to_owned(), binding));
        }
    }
    lifted
}

fn bind(holder: &mut Value, capability: String, binding: Value) {
    if !holder["bindings"].is_object() {
        holder["bindings"] = json!({});
    }
    if holder["bindings"][&capability].is_null() {
        holder["bindings"][capability] = binding;
    }
}

fn migrated(mut value: Value) -> Value {
    for (capability, binding) in lift(&mut value, &["tracker", "tests", "cloud"]) {
        bind(&mut value, capability, binding);
    }
    let shared = lift(&mut value, &["security"]);
    for (at, repository) in value["repositories"].as_array_mut().into_iter().flatten().enumerate() {
        for (capability, binding) in lift(repository, &["code", "security"]) {
            bind(repository, capability, binding);
        }
        if at == 0 {
            for (capability, binding) in shared.clone() {
                bind(repository, capability, binding);
            }
        }
    }
    value
}

fn read(data: &str) -> Result<Application> {
    Ok(serde_json::from_value(migrated(serde_json::from_str(data)?))?)
}

impl Store {
    pub fn applications(&self) -> Result<Vec<Application>> {
        let connection = self.locked();
        let mut statement = connection.prepare("SELECT data FROM applications ORDER BY position, rowid")?;
        let rows = statement.query_map([], |row| row.get::<_, String>(0))?.collect::<rusqlite::Result<Vec<_>>>()?;
        rows.iter().map(|data| read(data)).collect()
    }

    pub fn application(&self, id: &str) -> Result<Option<Application>> {
        let found: Option<String> = self
            .locked()
            .query_row("SELECT data FROM applications WHERE id = ?1", [id], |row| row.get(0))
            .optional()?;
        found.map(|data| read(&data)).transpose()
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

#[cfg(test)]
mod tests {
    use super::{Binding, Choice, read, watched};

    #[test]
    fn the_choices_saved_before_the_bindings_become_bindings_by_capability() {
        let old = r#"{"id":"a","name":"Web","skipDependabot":false,
            "tracker":{"connection":"c-jira","project":"APP"},
            "cloud":{"connection":"c-aws","account":"111","role":"Deploy"},
            "security":{"connection":"c-sec","project":"guid"},
            "repositories":[{"id":"r1","directory":"/web","code":{"connection":"c-gh","repo":"acme/web"}},
                            {"id":"r2","directory":"/api","run":"bun dev"}]}"#;
        let found = read(old).expect("parsed");
        assert_eq!(found.bound("tracker"), Some(&Binding::new("c-jira", "APP")));
        assert_eq!(found.bound("cloud"), Some(&Binding::new("c-aws", "111/Deploy")));
        assert_eq!(found.bound("security"), None);
        assert_eq!(found.repositories[0].bound("code"), Some(&Binding::new("c-gh", "acme/web")));
        assert_eq!(found.repositories[0].bound("security"), Some(&Binding::new("c-sec", "guid")));
        assert!(found.repositories[1].bindings.is_empty());
        assert_eq!(found.repositories[1].run, ["bun dev"]);
    }

    #[test]
    fn a_binding_already_on_the_repository_wins_over_the_old_shared_one() {
        let kept = read(
            r#"{"name":"Web","security":{"connection":"c","project":"old"},
            "repositories":[{"directory":"/web","bindings":{"security":{"connection":"c","value":"new"}}}]}"#,
        )
        .expect("parsed");
        assert_eq!(kept.repositories[0].bound("security").map(|binding| binding.value.as_str()), Some("new"));
    }

    #[test]
    fn every_value_bound_to_a_connection_is_watched_once() {
        let mut first = read(r#"{"name":"A","repositories":[]}"#).expect("parsed");
        let mut jira = Binding::new("c-jira", "APP");
        jira.more = vec![Choice { value: "OPS".into(), label: String::new() }];
        first.bindings.insert("tracker".into(), jira);
        let mut second = first.clone();
        second.bindings.insert("tracker".into(), Binding::new("c-jira", "OPS"));
        second.bindings.insert("tests".into(), Binding::new("c-zephyr", "QA"));
        assert_eq!(watched(&[first.clone(), second], "c-jira"), ["APP", "OPS"]);
        assert!(first.bindings["tracker"].holds("ops"));
        assert!(!first.bindings["tracker"].holds(""));
    }
}
