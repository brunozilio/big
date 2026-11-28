mod envelope;
mod lattice;
mod mark;
mod two;

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub use envelope::{TAG, envelope};
pub use lattice::{Levels, Mode, Pairing};
pub use mark::{Mark, Sink};
pub use two::{Held, Next};

use crate::workflow::WorkItem;

pub const READ: &str = "read";

pub type Reads = std::sync::Arc<std::sync::Mutex<Vec<Read>>>;

#[derive(Debug, Clone, Default)]
pub struct SessionReads(pub Reads);

impl PartialEq for SessionReads {
    fn eq(&self, other: &Self) -> bool {
        std::sync::Arc::ptr_eq(&self.0, &other.0)
    }
}

impl Eq for SessionReads {}

impl SessionReads {
    pub fn of(reads: &[Read]) -> Self {
        Self(std::sync::Arc::new(std::sync::Mutex::new(reads.to_vec())))
    }

    pub fn held(&self) -> Vec<Read> {
        self.0.lock().unwrap_or_else(std::sync::PoisonError::into_inner).clone()
    }

    pub fn keep(&self, read: Read) {
        let mut held = self.0.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        if !held.contains(&read) {
            held.push(read);
        }
    }
}

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
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub sensitive: bool,
    #[serde(default)]
    pub acted: bool,
}

impl Read {
    pub fn new(source: impl Into<String>, trust: Trust, label: Option<String>) -> Self {
        let label = label.map(|label| label.trim().to_owned()).filter(|label| !label.is_empty());
        Self { source: source.into(), trust, label, sensitive: false, acted: false }
    }

    pub fn exposed(self, sensitive: bool, acted: bool) -> Self {
        Self { sensitive: self.sensitive || sensitive, acted: self.acted || acted, ..self }
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
