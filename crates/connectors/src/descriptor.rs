use domain::connection::{Capability, ProviderKey};
use serde::Serialize;
use serde_json::{Map, Value, json};

use crate::oauth::{DeviceApp, Loopback, OAuthApp, SsoSpec};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Idempotency {
    Native,
    Lookup,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ActionSpec {
    pub name: String,
    pub input_schema: Value,
    pub output_schema: Value,
    pub writes: bool,
    pub idempotent: Idempotency,
}

impl ActionSpec {
    pub fn read(name: &str, input_schema: Value, output_schema: Value) -> ActionSpec {
        ActionSpec {
            name: name.to_owned(),
            input_schema,
            output_schema,
            writes: false,
            idempotent: Idempotency::Native,
        }
    }

    pub fn write(name: &str, input_schema: Value, output_schema: Value, idempotent: Idempotency) -> ActionSpec {
        ActionSpec { name: name.to_owned(), input_schema, output_schema, writes: true, idempotent }
    }

    pub fn capability(&self) -> Option<Capability> {
        Capability::of_action(&self.name)
    }

    pub fn missing(&self, args: &Value) -> Option<String> {
        let required = self.input_schema.get("required").and_then(Value::as_array)?;
        required
            .iter()
            .filter_map(Value::as_str)
            .find(|name| args.get(*name).is_none_or(Value::is_null))
            .map(str::to_owned)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthSpec {
    Redirect(OAuthApp),
    Device(DeviceApp),
    Sso(SsoSpec),
    Loopback(Loopback),
}

impl AuthSpec {
    pub fn kind(&self) -> &'static str {
        match self {
            AuthSpec::Redirect(_) => "redirect",
            AuthSpec::Device(_) => "device",
            AuthSpec::Sso(_) => "sso",
            AuthSpec::Loopback(_) => "loopback",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize)]
pub struct Presentation {
    pub description: String,
    pub group: String,
    pub button: String,
    pub site: String,
    pub choose_title: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AlternateAuth {
    pub key: String,
    pub label: String,
    pub button: String,
    pub site: String,
    pub auth: AuthSpec,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ProviderDescriptor {
    pub key: ProviderKey,
    pub name: String,
    pub logo: String,
    pub capabilities: Vec<Capability>,
    pub auth: AuthSpec,
    pub actions: Vec<ActionSpec>,
}

impl ProviderDescriptor {
    pub fn action(&self, name: &str) -> Option<&ActionSpec> {
        self.actions.iter().find(|action| action.name == name)
    }

    pub fn listing(&self) -> Value {
        json!({
            "key": self.key,
            "name": self.name,
            "logo": self.logo,
            "capabilities": self.capabilities,
            "auth": self.auth.kind(),
            "actions": self.actions,
        })
    }
}

pub fn object(required: &[(&str, &str)], optional: &[(&str, &str)]) -> Value {
    let properties: Map<String, Value> =
        required.iter().chain(optional).map(|(name, kind)| ((*name).to_owned(), json!({ "type": kind }))).collect();
    let names: Vec<&str> = required.iter().map(|(name, _)| *name).collect();
    json!({ "type": "object", "properties": properties, "required": names })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_schema_lists_its_required_fields_and_an_action_finds_the_missing_one() {
        let spec =
            ActionSpec::read("x.y", object(&[("a", "string"), ("b", "integer")], &[("c", "boolean")]), json!({}));
        assert_eq!(spec.input_schema["properties"]["c"]["type"], "boolean");
        assert_eq!(spec.missing(&json!({"a": "1", "b": 2})), None);
        assert_eq!(spec.missing(&json!({"a": "1", "b": null})), Some("b".to_owned()));
        assert_eq!(spec.capability(), Some(Capability::from("x")));
    }
}
