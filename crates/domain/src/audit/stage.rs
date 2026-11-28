use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FailureStage {
    Authentication,
    Authorization,
    Validation,
    Configuration,
    Dependency,
    Throttling,
    Timeout,
}

impl FailureStage {
    pub const ALL: [Self; 7] = [
        Self::Authentication,
        Self::Authorization,
        Self::Validation,
        Self::Configuration,
        Self::Dependency,
        Self::Throttling,
        Self::Timeout,
    ];

    pub const fn word(self) -> &'static str {
        match self {
            Self::Authentication => "authentication",
            Self::Authorization => "authorization",
            Self::Validation => "validation",
            Self::Configuration => "configuration",
            Self::Dependency => "dependency",
            Self::Throttling => "throttling",
            Self::Timeout => "timeout",
        }
    }

    pub fn named(word: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|stage| stage.word() == word)
    }

    pub fn of_kind(kind: &str) -> Option<Self> {
        match kind {
            "timeout" => Some(Self::Timeout),
            "transient" => Some(Self::Dependency),
            "rate_limited" => Some(Self::Throttling),
            "auth" => Some(Self::Authentication),
            "conflict" => Some(Self::Validation),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CreatorType {
    Person,
    Automation,
    Agent,
}

impl CreatorType {
    pub const fn word(self) -> &'static str {
        match self {
            Self::Person => "person",
            Self::Automation => "automation",
            Self::Agent => "agent",
        }
    }

    pub fn named(word: &str) -> Option<Self> {
        [Self::Person, Self::Automation, Self::Agent].into_iter().find(|kind| kind.word() == word)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum AuthorizedBy {
    Standing,
    Approval(String),
}

const STANDING: &str = "standing";
const APPROVAL: &str = "approval:";

impl fmt::Display for AuthorizedBy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Standing => f.write_str(STANDING),
            Self::Approval(id) => write!(f, "{APPROVAL}{id}"),
        }
    }
}

impl FromStr for AuthorizedBy {
    type Err = String;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        if text == STANDING {
            return Ok(Self::Standing);
        }
        match text.strip_prefix(APPROVAL).filter(|id| !id.is_empty()) {
            Some(id) => Ok(Self::Approval(id.to_owned())),
            None => Err(format!("{text} is neither {STANDING} nor {APPROVAL}<id>")),
        }
    }
}

impl Serialize for AuthorizedBy {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for AuthorizedBy {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        String::deserialize(deserializer)?.parse().map_err(serde::de::Error::custom)
    }
}
