use std::fmt;

use serde_json::Value;

pub const FIELD: &str = "schema_version";

pub type Migration = fn(&mut Value);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Contract {
    Connector,
    Workflow,
    Package,
    Trigger,
    Policy,
}

impl Contract {
    pub const ALL: [Self; 5] = [Self::Connector, Self::Workflow, Self::Package, Self::Trigger, Self::Policy];

    pub fn name(self) -> &'static str {
        match self {
            Self::Connector => "connector",
            Self::Workflow => "workflow",
            Self::Package => "package",
            Self::Trigger => "trigger",
            Self::Policy => "policy",
        }
    }

    pub fn current(self) -> u64 {
        u64::try_from(self.migrations().len()).unwrap_or(u64::MAX)
    }

    fn migrations(self) -> &'static [Migration] {
        match self {
            Self::Connector => &[stamped, stamped],
            Self::Workflow | Self::Package | Self::Trigger | Self::Policy => &[stamped],
        }
    }
}

fn stamped(_: &mut Value) {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SchemaError {
    NotAVersion { contract: Contract, at: String, found: String },
    Newer { contract: Contract, at: String, found: u64, known: u64 },
}

impl fmt::Display for SchemaError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotAVersion { contract, at, found } => {
                write!(f, "{at}: the {} {FIELD} must be a whole number, not {found}", contract.name())
            }
            Self::Newer { contract, at, found, known } => write!(
                f,
                "{at}: the {} declares {FIELD} {found}, newer than the {known} this version reads; update the app",
                contract.name()
            ),
        }
    }
}

impl std::error::Error for SchemaError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Read {
    pub found: Option<u64>,
    pub current: u64,
}

impl Read {
    pub fn migrated(&self) -> bool {
        self.found != Some(self.current)
    }
}

pub fn declared(contract: Contract, value: &Value, at: &str) -> Result<Option<u64>, SchemaError> {
    match value.get(FIELD) {
        None | Some(Value::Null) => Ok(None),
        Some(found) => found.as_u64().map(Some).ok_or_else(|| SchemaError::NotAVersion {
            contract,
            at: at.to_owned(),
            found: found.to_string(),
        }),
    }
}

pub fn read(contract: Contract, value: &mut Value, at: &str) -> Result<Read, SchemaError> {
    let found = declared(contract, value, at)?;
    let current = contract.current();
    let from = found.unwrap_or(0);
    if from > current {
        return Err(SchemaError::Newer { contract, at: at.to_owned(), found: from, known: current });
    }
    let steps = contract.migrations().iter().skip(usize::try_from(from).unwrap_or(usize::MAX));
    for migration in steps {
        migration(value);
    }
    if let Some(fields) = value.as_object_mut() {
        fields.insert(FIELD.to_owned(), Value::from(current));
    }
    let read = Read { found, current };
    if read.migrated() {
        let found = found.map_or_else(|| "none".to_owned(), |found| found.to_string());
        tracing::warn!(
            contract = contract.name(),
            at,
            found,
            current,
            "a contract declares an older {FIELD}; it was migrated on load, declare the current one"
        );
    }
    Ok(read)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn the_current_version_reads_as_is_and_a_missing_one_is_migrated() {
        for contract in Contract::ALL {
            let current = contract.current();
            let mut value = json!({ "name": "x", FIELD: current });
            let read = read(contract, &mut value, "a.ts").expect("current");
            assert!(!read.migrated());
            let mut old = json!({ "name": "x" });
            let read = super::read(contract, &mut old, "a.ts").expect("old");
            assert_eq!((read.found, read.migrated()), (None, true));
            assert_eq!(old[FIELD], json!(current));
        }
    }

    #[test]
    fn a_newer_or_a_broken_version_is_refused_with_its_place() {
        let newer = Contract::Workflow.current() + 1;
        let refused = read(Contract::Workflow, &mut json!({ FIELD: newer }), "workflows/x.ts");
        assert!(matches!(&refused, Err(SchemaError::Newer { found, .. }) if *found == newer));
        assert!(refused.err().is_some_and(|error| error.to_string().starts_with("workflows/x.ts: the workflow")));
        let broken = read(Contract::Trigger, &mut json!({ FIELD: "one" }), "t");
        assert!(matches!(broken, Err(SchemaError::NotAVersion { contract: Contract::Trigger, .. })));
    }
}
