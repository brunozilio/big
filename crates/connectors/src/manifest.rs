use std::fmt;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use content::schema::{Contract, SchemaError};
use domain::operation::{Kind, Problem, checked};
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ManifestError {
    Schema(SchemaError),
    WriteWithoutEffects { action: String },
    Attribute { action: String, problem: Problem },
    Migration(String),
}

impl fmt::Display for ManifestError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Schema(error) => write!(f, "{error}"),
            Self::WriteWithoutEffects { action } => {
                write!(f, "{action} writes but declares no effects: say what it changes (write, send-external, money…)")
            }
            Self::Attribute { action, problem } => write!(f, "{action} {problem}"),
            Self::Migration(error) => write!(f, "the connector could not be brought to the current contract: {error}"),
        }
    }
}

impl std::error::Error for ManifestError {}

fn acting(action: &Value) -> bool {
    action["effects"].as_array().into_iter().flatten().filter_map(Value::as_str).filter_map(Kind::named).any(Kind::acts)
}

fn action_checked(action: &Value) -> Result<(), ManifestError> {
    let name = action["name"].as_str().unwrap_or_default().to_owned();
    if action["writes"].as_bool() == Some(true) && !acting(action) {
        return Err(ManifestError::WriteWithoutEffects { action: name });
    }
    checked(action).map(drop).map_err(|problem| ManifestError::Attribute { action: name, problem })
}

pub fn migrated(root: Option<&Path>, declared: Value) -> Result<Value, ManifestError> {
    let catalog = crate::contracts::CATALOG;
    let source = content::user_text(root, catalog)
        .or_else(|| content::builtin_text(catalog))
        .ok_or_else(|| ManifestError::Migration(format!("{catalog} is missing")))?;
    let host: Arc<dyn script::Host> = Arc::new(crate::scripted::Local);
    let stop = Arc::new(AtomicBool::new(false));
    script::run_within(catalog, &source, "migrated", &[declared], &Value::Null, host, stop, None)
        .map_err(ManifestError::Migration)
}

pub fn read(declared: &mut Value, at: &str) -> Result<(), ManifestError> {
    content::schema::read(Contract::Connector, declared, at).map_err(ManifestError::Schema)?;
    declared["actions"].as_array().into_iter().flatten().try_for_each(action_checked)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn connector(action: Value) -> Value {
        json!({ "schema_version": 2, "name": "X", "actions": [action] })
    }

    fn profiled(name: &str, writes: bool, effects: Value) -> Value {
        json!({ "name": name, "writes": writes, "effects": effects, "risk": "low", "reversible": true, "untrusted": false, "sensitive": false })
    }

    #[test]
    fn a_write_without_effects_is_refused_with_a_typed_error() {
        let mut bare = connector(json!({ "name": "x.send", "writes": true, "risk": "high" }));
        assert_eq!(read(&mut bare, "x"), Err(ManifestError::WriteWithoutEffects { action: "x.send".into() }));
        let mut only_reads = connector(profiled("x.send", true, json!(["read"])));
        assert_eq!(read(&mut only_reads, "x"), Err(ManifestError::WriteWithoutEffects { action: "x.send".into() }));
        let mut sends = connector(profiled("x.send", true, json!(["send-external"])));
        assert_eq!(read(&mut sends, "x"), Ok(()));
    }

    #[test]
    fn every_action_declares_its_risk_attributes() {
        let mut read_only = connector(json!({ "name": "x.get", "writes": false, "effects": ["read"] }));
        assert_eq!(
            read(&mut read_only, "x"),
            Err(ManifestError::Attribute { action: "x.get".into(), problem: Problem::Missing("risk") })
        );
        let mut unknown = connector(profiled("x.get", false, json!(["teleport"])));
        assert!(matches!(read(&mut unknown, "x"), Err(ManifestError::Attribute { problem: Problem::Unknown(_), .. })));
        let mut newer = json!({ "schema_version": 99, "actions": [] });
        assert!(matches!(read(&mut newer, "x"), Err(ManifestError::Schema(_))));
    }

    #[test]
    fn an_older_connector_is_migrated_by_the_sdk_instead_of_refused() {
        let old = json!({ "name": "X", "actions": [{ "name": "x.send", "writes": true }, { "name": "x.key", "secret": true }] });
        let mut old = migrated(None, old).expect("migrated");
        assert_eq!(read(&mut old, "x"), Ok(()));
        assert_eq!((&old["actions"][0]["effects"], &old["actions"][0]["risk"]), (&json!(["write"]), &json!("medium")));
        assert_eq!(
            (&old["actions"][1]["effects"], &old["actions"][1]["visibility"]),
            (&json!(["read"]), &json!("host"))
        );
        let current = connector(json!({ "name": "x.send", "writes": true }));
        let mut current = migrated(None, current).expect("untouched");
        assert_eq!(read(&mut current, "x"), Err(ManifestError::WriteWithoutEffects { action: "x.send".into() }));
    }
}
