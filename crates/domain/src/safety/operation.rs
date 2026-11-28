use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Value;

pub const META: &str = "operation";

pub const REQUIRED: [&str; 5] = ["effects", "risk", "reversible", "untrusted", "sensitive"];

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

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Risk {
    Low,
    #[default]
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

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum Reversible {
    Yes,
    #[default]
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
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Permission {
    pub default: Initial,
    pub always_ask: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(default)]
pub struct Profile {
    pub effects: Vec<Kind>,
    pub risk: Risk,
    pub reversible: Reversible,
    pub untrusted: bool,
    pub sensitive: bool,
}

impl Profile {
    pub fn conservative() -> Self {
        Self {
            effects: vec![Kind::Write, Kind::SendExternal],
            risk: Risk::High,
            reversible: Reversible::No,
            untrusted: true,
            sensitive: true,
        }
    }

    pub fn acts(&self) -> bool {
        self.effects.iter().any(|kind| kind.acts())
    }

    pub fn strict(&self) -> bool {
        self.risk == Risk::High || self.effects.iter().any(|kind| kind.strict())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Problem {
    Missing(&'static str),
    NoEffects,
    Unknown(String),
    Invalid { attribute: &'static str, why: String },
}

impl std::fmt::Display for Problem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Missing(attribute) => write!(f, "declares no {attribute}"),
            Self::NoEffects => write!(f, "declares an empty effects list"),
            Self::Unknown(effect) => write!(f, "declares the unknown effect {effect}"),
            Self::Invalid { attribute, why } => write!(f, "declares an invalid {attribute}: {why}"),
        }
    }
}

pub fn checked(declared: &Value) -> Result<Profile, Problem> {
    if let Some(missing) = REQUIRED.into_iter().find(|attribute| declared.get(*attribute).is_none_or(Value::is_null)) {
        return Err(Problem::Missing(missing));
    }
    let listed =
        declared["effects"].as_array().ok_or(Problem::Invalid { attribute: "effects", why: "not a list".into() })?;
    if listed.is_empty() {
        return Err(Problem::NoEffects);
    }
    if let Some(unknown) = listed.iter().find(|effect| effect.as_str().and_then(Kind::named).is_none()) {
        return Err(Problem::Unknown(unknown.to_string()));
    }
    for flag in ["untrusted", "sensitive"] {
        if !declared[flag].is_boolean() {
            return Err(Problem::Invalid { attribute: flag, why: "not true or false".into() });
        }
    }
    serde_json::from_value(declared.clone())
        .map_err(|error| Problem::Invalid { attribute: "profile", why: error.to_string() })
}

pub fn values_at(input: &Value, keys: &[String]) -> Vec<String> {
    let mut found = Vec::new();
    for key in keys {
        match &input[key.as_str()] {
            Value::String(text) if !text.trim().is_empty() => found.push(text.trim().to_owned()),
            Value::Array(list) => {
                found.extend(
                    list.iter()
                        .filter_map(Value::as_str)
                        .map(str::trim)
                        .filter(|text| !text.is_empty())
                        .map(str::to_owned),
                );
            }
            _ => {}
        }
    }
    found.dedup();
    found
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Declaration {
    #[serde(flatten)]
    pub profile: Profile,
    pub recipients: Vec<String>,
    pub free_text: Vec<String>,
    pub permission: Permission,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Operation {
    pub server: String,
    pub effects: Vec<Kind>,
    pub risk: Risk,
    pub recipients: Vec<String>,
    pub permission: Permission,
}

impl Operation {
    pub fn acts(&self) -> bool {
        self.effects.iter().any(|kind| kind.acts())
    }

    pub fn strict(&self) -> bool {
        self.permission.always_ask || self.risk == Risk::High || self.effects.iter().any(|kind| kind.strict())
    }

    pub fn free(&self) -> bool {
        self.permission.default == Initial::Allow && !self.strict()
    }
}

impl Declaration {
    pub fn operation(&self, server: &str, input: &Value) -> Option<Operation> {
        self.profile.acts().then(|| Operation {
            server: server.to_owned(),
            effects: self.profile.effects.clone(),
            risk: self.profile.risk,
            recipients: values_at(input, &self.recipients),
            permission: self.permission,
        })
    }
}

pub fn hinted(server: &str, annotations: &Value) -> Option<Operation> {
    (annotations["destructiveHint"].as_bool() == Some(true)).then(|| Operation {
        server: server.to_owned(),
        effects: vec![Kind::Destructive],
        risk: Risk::High,
        recipients: Vec::new(),
        permission: Permission { default: Initial::Ask, always_ask: true },
    })
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Disclosed {
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub effects: Vec<Kind>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub risk: Option<Risk>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub recipients: Vec<String>,
}

impl Disclosed {
    pub fn of<'a>(operations: impl IntoIterator<Item = &'a Operation>) -> Self {
        let mut disclosed = Self::default();
        for operation in operations {
            disclosed.effects.extend(operation.effects.iter().copied());
            disclosed.recipients.extend(operation.recipients.iter().cloned());
            disclosed.risk = disclosed.risk.max(Some(operation.risk));
        }
        disclosed.effects.sort();
        disclosed.effects.dedup();
        disclosed.recipients.dedup();
        disclosed
    }

    pub fn is_empty(&self) -> bool {
        self.effects.is_empty() && self.risk.is_none() && self.recipients.is_empty()
    }
}
#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn a_declaration_names_its_effects_and_finds_the_recipients_in_the_arguments() {
        let declared: Declaration = serde_json::from_value(json!({
            "effects": ["send-external", "pii"], "risk": "high", "reversible": false, "untrusted": false,
            "sensitive": true, "recipients": ["to", "cc"], "freeText": ["body"], "permission": { "alwaysAsk": true },
        }))
        .expect("declaration");
        assert_eq!(declared.profile.reversible, Reversible::No);
        let input = json!({ "to": ["ana@example.com", " "], "cc": "rui@example.com", "body": "oi" });
        let operation = declared.operation("s", &input).expect("acts");
        assert_eq!(operation.recipients, ["ana@example.com", "rui@example.com"]);
        assert!(operation.strict());
        let disclosed = Disclosed::of([&operation]);
        assert_eq!((disclosed.effects, disclosed.risk), (vec![Kind::SendExternal, Kind::Pii], Some(Risk::High)));
        let reading: Declaration = serde_json::from_value(json!({ "effects": ["read", "pii"] })).expect("read");
        assert_eq!(reading.operation("s", &input), None);
    }

    #[test]
    fn a_profile_is_checked_attribute_by_attribute() {
        let full = json!({ "effects": ["write"], "risk": "low", "reversible": "compensate", "untrusted": false, "sensitive": false });
        assert_eq!(checked(&full).map(|profile| profile.reversible), Ok(Reversible::Compensate));
        let mut missing = full.clone();
        missing["sensitive"] = Value::Null;
        assert_eq!(checked(&missing), Err(Problem::Missing("sensitive")));
        let empty = json!({ "effects": [], "risk": "low", "reversible": true, "untrusted": false, "sensitive": false });
        assert_eq!(checked(&empty), Err(Problem::NoEffects));
        let mut odd = full;
        odd["reversible"] = json!("maybe");
        assert!(matches!(checked(&odd), Err(Problem::Invalid { .. })));
        assert_eq!(serde_json::to_value(Reversible::Compensate).ok(), Some(json!("compensate")));
    }

    #[test]
    fn a_third_party_destructive_hint_always_asks() {
        let destructive = hinted("other", &json!({ "destructiveHint": true })).expect("destructive");
        assert!(destructive.strict() && destructive.acts());
        assert_eq!(hinted("other", &json!({ "readOnlyHint": true })), None);
    }
}
