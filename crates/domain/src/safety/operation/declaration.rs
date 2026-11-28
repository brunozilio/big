use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{Initial, Kind, Permission, Reversible, Risk};

pub const META: &str = "operation";

pub const REQUIRED: [&str; 5] = ["effects", "risk", "reversible", "untrusted", "sensitive"];

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Profile {
    pub effects: Vec<Kind>,
    pub risk: Risk,
    pub reversible: Reversible,
    pub untrusted: bool,
    pub sensitive: bool,
}

impl Profile {
    pub fn new(effects: Vec<Kind>, risk: Risk, reversible: Reversible, untrusted: bool, sensitive: bool) -> Self {
        Self { effects, risk, reversible, untrusted, sensitive }
    }

    pub fn conservative() -> Self {
        Self::new(vec![Kind::Write, Kind::SendExternal], Risk::High, Reversible::No, true, true)
    }

    pub fn acts(&self) -> bool {
        self.effects.iter().any(|kind| kind.acts())
    }

    pub fn strict(&self) -> bool {
        self.risk == Risk::High || self.effects.iter().any(|kind| kind.strict())
    }

    pub fn operation(&self, server: &str) -> Operation {
        Operation {
            server: server.to_owned(),
            effects: self.effects.clone(),
            risk: self.risk,
            recipients: Vec::new(),
            permission: Permission::default(),
        }
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
    let profile: Profile = serde_json::from_value(declared.clone())
        .map_err(|error| Problem::Invalid { attribute: "profile", why: error.to_string() })?;
    if profile.effects.contains(&Kind::Pii) && !profile.sensitive {
        return Err(Problem::Invalid { attribute: "sensitive", why: "personal data (pii) is sensitive".into() });
    }
    Ok(profile)
}

pub fn values_at(input: &Value, keys: &[String]) -> Vec<String> {
    let mut found: Vec<String> = Vec::new();
    for key in keys {
        let texts: Vec<&str> = match &input[key.as_str()] {
            Value::String(text) => vec![text.as_str()],
            Value::Array(list) => list.iter().filter_map(Value::as_str).collect(),
            _ => Vec::new(),
        };
        for text in texts.into_iter().map(str::trim).filter(|text| !text.is_empty()) {
            if !found.iter().any(|known| known == text) {
                found.push(text.to_owned());
            }
        }
    }
    found
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Declaration {
    #[serde(flatten)]
    pub profile: Profile,
    #[serde(default)]
    pub recipients: Vec<String>,
    #[serde(default)]
    pub free_text: Vec<String>,
    #[serde(default)]
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

    pub fn refused(&self) -> bool {
        self.permission.default == Initial::Deny
    }
}

impl Declaration {
    pub fn operation(&self, server: &str, input: &Value) -> Option<Operation> {
        let refused = self.permission.default == Initial::Deny;
        (self.profile.acts() || refused).then(|| Operation {
            recipients: values_at(input, &self.recipients),
            permission: self.permission,
            ..self.profile.operation(server)
        })
    }
}

pub fn hinted(server: &str, annotations: &Value) -> Option<Operation> {
    (annotations["destructiveHint"].as_bool() == Some(true)).then(|| Operation {
        permission: Permission { default: Initial::Ask, always_ask: true },
        ..Profile::new(vec![Kind::Destructive], Risk::High, Reversible::No, true, false).operation(server)
    })
}
