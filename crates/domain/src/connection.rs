use std::fmt;

use serde::{Deserialize, Serialize};

macro_rules! text_id {
    ($name:ident) => {
        #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(pub String);

        impl $name {
            pub fn new(value: impl Into<String>) -> Self {
                Self(value.into())
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str(&self.0)
            }
        }

        impl From<&str> for $name {
            fn from(value: &str) -> Self {
                Self(value.to_owned())
            }
        }
    };
}

text_id!(ProviderKey);
text_id!(ConnectionId);
text_id!(Capability);

impl Capability {
    pub fn of_action(action: &str) -> Option<Capability> {
        action.split_once('.').map(|(capability, _)| capability).filter(|name| !name.is_empty()).map(Capability::from)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Connection {
    pub id: ConnectionId,
    pub provider: ProviderKey,
    pub label: String,
    pub account: String,
    pub capabilities: Vec<Capability>,
}

impl Connection {
    pub fn offers(&self, capability: &Capability) -> bool {
        self.capabilities.contains(capability)
    }
}

pub fn default_for<'a>(connections: &'a [Connection], capability: &Capability) -> Option<&'a Connection> {
    connections.iter().find(|connection| connection.offers(capability))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn connection(id: &str, capabilities: &[&str]) -> Connection {
        Connection {
            id: ConnectionId::from(id),
            provider: ProviderKey::from("p"),
            label: id.to_owned(),
            account: "a".to_owned(),
            capabilities: capabilities.iter().map(|name| Capability::from(*name)).collect(),
        }
    }

    #[test]
    fn the_capability_is_the_prefix_of_an_action() {
        assert_eq!(Capability::of_action("tracker.transition"), Some(Capability::from("tracker")));
        assert_eq!(Capability::of_action("plain"), None);
        assert_eq!(Capability::of_action(".x"), None);
    }

    #[test]
    fn several_connections_of_one_capability_live_together_and_the_first_is_the_default() {
        let all = [connection("one", &["code", "pipelines"]), connection("two", &["code"])];
        let code = Capability::from("code");
        assert_eq!(all.iter().filter(|c| c.offers(&code)).count(), 2);
        assert_eq!(default_for(&all, &code).map(|c| c.id.as_str()), Some("one"));
        assert!(default_for(&all, &Capability::from("tracker")).is_none());
    }

    #[test]
    fn ids_serialize_as_plain_strings() {
        let text = serde_json::to_string(&connection("c1", &["tracker"])).unwrap_or_default();
        assert!(text.contains("\"id\":\"c1\""));
        assert!(text.contains("\"capabilities\":[\"tracker\"]"));
    }
}
