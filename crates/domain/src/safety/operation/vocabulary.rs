use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Kind {
    Read,
    Write,
    SendExternal,
    Money,
    Pii,
    Legal,
    Destructive,
    Memory,
    ConsumesBudget,
}

impl Kind {
    pub const ALL: [Self; 9] = [
        Self::Read,
        Self::Write,
        Self::SendExternal,
        Self::Money,
        Self::Pii,
        Self::Legal,
        Self::Destructive,
        Self::Memory,
        Self::ConsumesBudget,
    ];

    pub fn word(self) -> &'static str {
        match self {
            Self::Read => "read",
            Self::Write => "write",
            Self::SendExternal => "send-external",
            Self::Money => "money",
            Self::Pii => "pii",
            Self::Legal => "legal",
            Self::Destructive => "destructive",
            Self::Memory => "memory",
            Self::ConsumesBudget => "consumes-budget",
        }
    }

    pub fn named(word: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.word() == word)
    }

    pub fn acts(self) -> bool {
        !matches!(self, Self::Read | Self::Pii | Self::ConsumesBudget)
    }

    pub fn strict(self) -> bool {
        matches!(self, Self::SendExternal | Self::Money | Self::Legal | Self::Destructive)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Risk {
    Low,
    Medium,
    High,
}

impl Risk {
    pub fn word(self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Reversible {
    Yes,
    No,
    Compensate,
}

const COMPENSATE: &str = "compensate";

impl Serialize for Reversible {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Yes => serializer.serialize_bool(true),
            Self::No => serializer.serialize_bool(false),
            Self::Compensate => serializer.serialize_str(COMPENSATE),
        }
    }
}

impl<'de> Deserialize<'de> for Reversible {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        match Value::deserialize(deserializer)? {
            Value::Bool(true) => Ok(Self::Yes),
            Value::Bool(false) => Ok(Self::No),
            Value::String(word) if word == COMPENSATE => Ok(Self::Compensate),
            other => {
                Err(serde::de::Error::custom(format!("reversible is true, false or \"{COMPENSATE}\", not {other}")))
            }
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Visibility {
    #[default]
    Model,
    Host,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Initial {
    #[default]
    Ask,
    Allow,
    Deny,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Permission {
    pub default: Initial,
    pub always_ask: bool,
}
