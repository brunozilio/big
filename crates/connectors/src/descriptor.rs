use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use domain::Language;
use domain::connection::{Capability, ProviderKey};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

use crate::oauth::{DeviceApp, Loopback, Stepped};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Idempotency {
    Native,
    Lookup,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActionSpec {
    pub name: String,
    #[serde(default)]
    pub input_schema: Value,
    #[serde(default)]
    pub output_schema: Value,
    #[serde(default)]
    pub writes: bool,
    #[serde(default = "native")]
    pub idempotent: Idempotency,
}

fn native() -> Idempotency {
    Idempotency::Native
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
    Device(DeviceApp),
    Steps(Stepped),
    Loopback(Loopback),
    Token(TokenSpec),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TokenForm {
    #[default]
    Bearer,
    Basic,
    Json,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct TokenField {
    pub name: String,
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub help: String,
    #[serde(default)]
    pub secret: bool,
    #[serde(default)]
    pub setting: bool,
    #[serde(default)]
    pub value: String,
    #[serde(default)]
    pub placeholder: String,
    #[serde(default)]
    pub optional: bool,
}

impl TokenField {
    pub fn new(name: &str, label: &str, help: &str, secret: bool) -> TokenField {
        TokenField {
            name: name.to_owned(),
            label: label.to_owned(),
            help: help.to_owned(),
            secret,
            setting: false,
            value: String::new(),
            placeholder: String::new(),
            optional: false,
        }
    }

    pub fn filled(self, value: &str) -> TokenField {
        TokenField { value: value.to_owned(), ..self }
    }

    pub fn setting(name: &str, label: &str, help: &str) -> TokenField {
        TokenField { setting: true, ..TokenField::new(name, label, help, false) }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct TokenSpec {
    pub fields: Vec<TokenField>,
    #[serde(default)]
    pub form: TokenForm,
}

impl TokenSpec {
    pub fn access_token(&self, given: &Value, language: Language) -> Result<String, String> {
        let mut texts = Vec::new();
        for field in self.fields.iter().filter(|field| !field.setting) {
            let value = given[&field.name].as_str().map(str::trim).unwrap_or_default();
            if value.is_empty() && !field.optional {
                return Err(crate::texts::missing_field(language, &field.label));
            }
            texts.push((field.name.as_str(), value));
        }
        let values: Vec<&str> = texts.iter().map(|(_, value)| *value).collect();
        Ok(match self.form {
            TokenForm::Bearer => values.concat(),
            TokenForm::Basic => format!("Basic {}", STANDARD.encode(values.join(":"))),
            TokenForm::Json => {
                Value::Object(texts.iter().map(|(name, value)| ((*name).to_owned(), json!(value))).collect())
                    .to_string()
            }
        })
    }
}

impl TokenSpec {
    pub fn settings(&self, given: &Value) -> Value {
        let kept = self.fields.iter().filter(|field| field.setting).filter_map(|field| {
            let value = given[&field.name].as_str().map(str::trim).filter(|value| !value.is_empty())?;
            Some((field.name.clone(), json!(value)))
        });
        Value::Object(kept.collect())
    }
}

impl AuthSpec {
    pub fn kind(&self) -> &'static str {
        match self {
            AuthSpec::Device(_) => "device",
            AuthSpec::Steps(_) => "steps",
            AuthSpec::Loopback(_) => "loopback",
            AuthSpec::Token(_) => "token",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Presentation {
    pub description: String,
    pub group: String,
    pub button: String,
    pub site: String,
    pub choose_title: String,
    pub hidden: bool,
    pub bindings: Vec<BindingSpec>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct BindingSpec {
    pub capability: String,
    pub scope: String,
    pub label: String,
    pub multiple: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub fields: Vec<BindingField>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub each: Option<BindingEach>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub section: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub order: Option<serde_json::Number>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub suggests: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct BindingField {
    pub key: String,
    pub label: String,
    pub required: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct BindingEach {
    pub key: String,
    pub label: String,
    pub fields: Vec<BindingField>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AlternateAuth {
    pub key: String,
    pub label: String,
    pub button: String,
    pub site: String,
    pub auth: AuthSpec,
}

impl AlternateAuth {
    pub fn token(
        language: Language,
        key: &str,
        label: &str,
        site: &str,
        fields: Vec<TokenField>,
        form: TokenForm,
    ) -> AlternateAuth {
        AlternateAuth {
            key: key.to_owned(),
            label: label.to_owned(),
            button: crate::texts::connect(language).to_owned(),
            site: site.to_owned(),
            auth: AuthSpec::Token(TokenSpec { fields, form }),
        }
    }
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

fn of_kind(kind: &str, value: &Value) -> bool {
    match kind {
        "string" => value.is_string(),
        "integer" => value.is_i64() || value.is_u64(),
        "number" => value.is_number(),
        "boolean" => value.is_boolean(),
        "array" => value.is_array(),
        "object" => value.is_object(),
        _ => true,
    }
}

pub fn conforms(schema: &Value, value: &Value) -> Result<(), String> {
    let kind = schema["type"].as_str().unwrap_or_default();
    if !of_kind(kind, value) {
        return Err(format!("expected {kind}, got {value}"));
    }
    let required: Vec<&str> = schema["required"].as_array().into_iter().flatten().filter_map(Value::as_str).collect();
    let properties = schema["properties"].as_object().into_iter().flatten();
    for (name, property) in properties {
        let field = &value[name.as_str()];
        let needed = required.contains(&name.as_str());
        if field.is_null() && !needed {
            continue;
        }
        let kind = property["type"].as_str().unwrap_or_default();
        if !of_kind(kind, field) {
            return Err(format!("`{name}` should be {kind}, got {field}"));
        }
    }
    Ok(())
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

    #[test]
    fn typed_credentials_become_a_bearer_a_basic_or_a_json_token() {
        let field = |name: &str| TokenField::new(name, name, "", true);
        let spec = |form| TokenSpec { fields: vec![field("email"), field("token")], form };
        let given = json!({ "email": " ana@x.com ", "token": "t1" });
        assert_eq!(
            spec(TokenForm::Basic).access_token(&given, domain::Language::Portuguese),
            Ok(format!("Basic {}", STANDARD.encode("ana@x.com:t1")))
        );
        let keys = spec(TokenForm::Json).access_token(&given, domain::Language::Portuguese).unwrap_or_default();
        assert_eq!(serde_json::from_str::<Value>(&keys).ok(), Some(json!({ "email": "ana@x.com", "token": "t1" })));
        let single = TokenSpec { fields: vec![field("token")], form: TokenForm::Bearer };
        assert_eq!(single.access_token(&given, domain::Language::Portuguese), Ok("t1".to_owned()));
        let sited = TokenSpec {
            fields: vec![TokenField::setting("site", "site", ""), field("token")],
            form: TokenForm::Bearer,
        };
        assert_eq!(sited.access_token(&given, domain::Language::Portuguese), Ok("t1".to_owned()));
        assert_eq!(sited.settings(&json!({ "site": " eu ", "token": "t1" })), json!({ "site": "eu" }));
        assert_eq!(sited.settings(&json!({ "site": " ", "token": "t1" })), json!({}));
        assert_eq!(
            spec(TokenForm::Basic).access_token(&json!({ "email": "a" }), domain::Language::Portuguese),
            Err("Informe o token.".to_owned())
        );
        assert_eq!(
            spec(TokenForm::Basic).access_token(&json!({ "email": "a" }), domain::Language::English),
            Err("Enter the token.".to_owned())
        );
    }
}
