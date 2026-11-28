mod envelope;
mod lattice;
mod mark;

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub use envelope::{TAG, envelope};
pub use lattice::{Levels, Mode};
pub use mark::{Mark, Sink};

use crate::workflow::WorkItem;

pub const READ: &str = "read";

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Trust {
    UserAuthored,
    External,
    #[default]
    System,
}

impl Trust {
    pub fn word(self) -> &'static str {
        match self {
            Self::UserAuthored => "user-authored",
            Self::External => "external",
            Self::System => "system",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Read {
    pub source: String,
    pub trust: Trust,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

impl Read {
    pub fn new(source: impl Into<String>, trust: Trust, label: Option<String>) -> Self {
        let label = label.map(|label| label.trim().to_owned()).filter(|label| !label.is_empty());
        Self { source: source.into(), trust, label }
    }

    pub fn progress(&self) -> Value {
        serde_json::json!({ READ: self })
    }
}

pub fn reads(item: &WorkItem) -> Vec<Read> {
    item.effects
        .values()
        .filter_map(|entry| serde_json::from_value::<Read>(entry.progress.get(READ)?.clone()).ok())
        .collect()
}

#[cfg(test)]
mod tests;
