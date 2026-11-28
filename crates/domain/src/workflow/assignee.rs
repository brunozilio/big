use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Assignee {
    Person(String),
    Group(String),
    Role(String),
    Admins,
}

impl Assignee {
    pub fn read(value: &Value) -> Option<Self> {
        let read: Self = serde_json::from_value(value.clone()).ok()?;
        match &read {
            Self::Person(name) | Self::Group(name) | Self::Role(name) if name.trim().is_empty() => None,
            Self::Person(name) => Some(Self::Person(name.trim().to_owned())),
            Self::Group(name) => Some(Self::Group(name.trim().to_owned())),
            Self::Role(name) => Some(Self::Role(name.trim().to_owned())),
            Self::Admins => Some(read),
        }
    }

    pub fn kind(&self) -> &'static str {
        match self {
            Self::Person(_) => "person",
            Self::Group(_) => "group",
            Self::Role(_) => "role",
            Self::Admins => "admins",
        }
    }

    pub fn name(&self) -> &str {
        match self {
            Self::Person(name) | Self::Group(name) | Self::Role(name) => name,
            Self::Admins => "",
        }
    }

    pub fn shown(&self) -> Value {
        json!({ "kind": self.kind(), "name": self.name() })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_assignee_is_read_from_what_the_sdk_writes() {
        assert_eq!(Assignee::read(&json!({ "role": " controller " })), Some(Assignee::Role("controller".into())));
        assert_eq!(
            Assignee::read(&json!({ "person": "ana@example.com" })),
            Some(Assignee::Person("ana@example.com".into()))
        );
        assert_eq!(Assignee::read(&json!({ "group": "financeiro" })), Some(Assignee::Group("financeiro".into())));
        assert_eq!(Assignee::read(&json!("admins")), Some(Assignee::Admins));
        assert_eq!(Assignee::read(&json!({ "role": "" })), None);
        assert_eq!(Assignee::read(&Value::Null), None);
        assert_eq!(Assignee::read(&json!({ "team": "x" })), None);
        assert_eq!(Assignee::Role("x".into()).shown(), json!({ "kind": "role", "name": "x" }));
    }
}
