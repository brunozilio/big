use std::collections::BTreeMap;

use async_trait::async_trait;
use domain::Language;
use domain::connection::{Capability, ConnectionId};
use jiff::Timestamp;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::descriptor::{AlternateAuth, AuthSpec, Presentation, ProviderDescriptor};
use crate::error::ActionError;
use crate::http::Http;

#[derive(Debug, Clone, PartialEq)]
pub struct Credentials {
    pub connection: ConnectionId,
    pub access_token: String,
    pub settings: Value,
}

impl Credentials {
    pub fn new(connection: &ConnectionId, access_token: &str, settings: Value) -> Credentials {
        Credentials { connection: connection.clone(), access_token: access_token.to_owned(), settings }
    }

    pub fn setting(&self, name: &str) -> Option<&str> {
        self.settings.get(name).and_then(Value::as_str).filter(|value| !value.is_empty())
    }

    pub fn settings_list(&self, name: &str) -> Vec<String> {
        self.settings
            .get(name)
            .and_then(Value::as_array)
            .map(|items| items.iter().filter_map(Value::as_str).map(str::to_owned).collect())
            .unwrap_or_default()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AccountOption {
    pub id: String,
    pub label: String,
    pub settings: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExternalEvent {
    pub id: String,
    pub connection: ConnectionId,
    pub capability: Capability,
    pub kind: String,
    pub subject: String,
    pub data: Value,
    pub at: Timestamp,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub scope: String,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct EventBatch {
    pub events: Vec<ExternalEvent>,
    pub cursor: String,
}

#[async_trait]
pub trait EventSource: Send + Sync + std::fmt::Debug {
    async fn poll(
        &self,
        http: &dyn Http,
        creds: &Credentials,
        cursor: Option<&str>,
        watch: &[String],
    ) -> Result<EventBatch, ActionError>;
}

#[async_trait]
pub trait SecretSource: Send + Sync + std::fmt::Debug {
    async fn secret(&self, http: &dyn Http, reference: &str) -> Result<String, ActionError>;
}

#[async_trait]
pub trait Provider: Send + Sync + std::fmt::Debug {
    fn descriptor(&self) -> ProviderDescriptor;

    async fn call(
        &self,
        http: &dyn Http,
        creds: &Credentials,
        action: &str,
        args: &Value,
        idempotency_key: &str,
    ) -> Result<Value, ActionError>;

    async fn accounts(&self, http: &dyn Http, creds: &Credentials) -> Result<Vec<AccountOption>, ActionError>;

    fn events(&self) -> Option<&dyn EventSource> {
        None
    }

    fn secrets(&self) -> Option<&dyn SecretSource> {
        None
    }

    fn presentation(&self, _language: Language) -> Presentation {
        Presentation::default()
    }

    fn alternatives(&self, _language: Language) -> Vec<AlternateAuth> {
        Vec::new()
    }

    fn chooses(&self) -> Vec<String> {
        Vec::new()
    }

    fn requires(&self) -> domain::data_policy::Requires {
        domain::data_policy::Requires::default()
    }

    fn messages(&self, _language: Language) -> BTreeMap<String, String> {
        BTreeMap::new()
    }

    fn secret_patterns(&self) -> domain::redact::Rules {
        domain::redact::Rules::default()
    }
}

pub trait SettingsSink: Send + Sync + std::fmt::Debug {
    fn learned(&self, connection: &ConnectionId, settings: &Value);
}

pub fn auth_for(primary: &AuthSpec, alternatives: &[AlternateAuth], method: Option<&str>) -> AuthSpec {
    let chosen = method.and_then(|method| alternatives.iter().find(|alternate| alternate.key == method));
    chosen.map_or_else(|| primary.clone(), |alternate| alternate.auth.clone())
}

pub fn unknown_action(action: &str) -> ActionError {
    ActionError::permanent(format!("the action `{action}` is not offered here"))
}

#[derive(Debug, Clone, Copy)]
pub struct Args<'a>(pub &'a Value);

impl<'a> Args<'a> {
    pub fn text(&self, name: &str) -> Result<&'a str, ActionError> {
        self.optional_text(name).ok_or_else(|| ActionError::missing(name))
    }

    pub fn optional_text(&self, name: &str) -> Option<&'a str> {
        self.0.get(name).and_then(Value::as_str).filter(|value| !value.is_empty())
    }

    pub fn number(&self, name: &str) -> Result<u64, ActionError> {
        self.optional_number(name).ok_or_else(|| ActionError::missing(name))
    }

    pub fn optional_number(&self, name: &str) -> Option<u64> {
        let value = self.0.get(name)?;
        value.as_u64().or_else(|| value.as_str().and_then(|text| text.parse().ok()))
    }

    pub fn optional_flag(&self, name: &str) -> Option<bool> {
        self.0.get(name).and_then(Value::as_bool)
    }

    pub fn flag(&self, name: &str) -> Result<bool, ActionError> {
        self.0.get(name).and_then(Value::as_bool).ok_or_else(|| ActionError::missing(name))
    }

    pub fn list(&self, name: &str) -> &'a [Value] {
        self.0.get(name).and_then(Value::as_array).map_or(&[], Vec::as_slice)
    }
}

pub fn text_of(value: &Value, pointer: &str) -> String {
    value.pointer(pointer).and_then(Value::as_str).unwrap_or_default().to_owned()
}

pub fn timestamp(text: &str) -> Option<Timestamp> {
    text.parse::<Timestamp>()
        .ok()
        .or_else(|| jiff::fmt::strtime::parse("%Y-%m-%dT%H:%M:%S%.f%z", text).and_then(|time| time.to_timestamp()).ok())
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn arguments_read_text_numbers_and_flags() {
        let value = json!({"a": "x", "n": 3, "s": "7", "f": true, "l": [1], "e": ""});
        let args = Args(&value);
        assert_eq!(args.text("a"), Ok("x"));
        assert!(args.text("e").is_err());
        assert_eq!(args.number("n"), Ok(3));
        assert_eq!(args.number("s"), Ok(7));
        assert_eq!(args.flag("f"), Ok(true));
        assert_eq!(args.list("l").len(), 1);
        assert!(args.list("none").is_empty());
    }

    #[test]
    fn timestamps_read_both_iso_and_offset_without_colon() {
        assert_eq!(timestamp("2024-01-02T03:04:05Z").map(|t| t.as_second()), Some(1_704_164_645));
        assert_eq!(timestamp("2024-01-02T03:04:05.000+0000").map(|t| t.as_second()), Some(1_704_164_645));
        assert_eq!(timestamp("nope"), None);
    }
}
