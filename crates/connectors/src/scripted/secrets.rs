use std::sync::Arc;

use async_trait::async_trait;
use serde_json::{Map, Value, json};

use super::bridge::{Answers, Script, invoke};
use super::host::{Local, Online};
use super::sessions::McpSessions;
use super::sign_in::redirect;
use crate::error::ActionError;
use crate::http::Http;
use crate::provider::SecretSource;

pub const READ: &str = "secret";
pub const ENVIRONMENT: &str = "secretEnvironment";

#[derive(Debug)]
pub struct ScriptedSecrets {
    pub script: Script,
    pub sessions: Arc<McpSessions>,
}

impl ScriptedSecrets {
    fn environment(&self) -> Result<Value, ActionError> {
        if self.script.kind(ENVIRONMENT) != "function" {
            return Ok(json!({}));
        }
        let names = self.script.invoke_here(ENVIRONMENT, &json!({}), Arc::new(Local))?;
        let names: Vec<String> = serde_json::from_value(names).map_err(ActionError::decode)?;
        let found: Map<String, Value> = names
            .into_iter()
            .filter_map(|name| platform::environment::var(&name).map(|value| (name, Value::String(value))))
            .collect();
        Ok(Value::Object(found))
    }
}

#[async_trait]
impl SecretSource for ScriptedSecrets {
    async fn secret(&self, http: &dyn Http, reference: &str) -> Result<String, ActionError> {
        let input = json!({ "reference": reference, "environment": self.environment()? });
        let online = Online::pooled(http, redirect(), Arc::clone(&self.sessions), &input);
        let answers: Arc<dyn Answers> = Arc::new(Local);
        let value = invoke(&self.script, answers, &online, READ, input).await?;
        value.as_str().map(str::to_owned).ok_or_else(|| ActionError::decode("the secret is not text"))
    }
}
