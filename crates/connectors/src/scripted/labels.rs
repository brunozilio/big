use std::sync::Arc;

use serde_json::{Value, json};

use super::bridge::Script;
use super::host::Local;

const LABELS: &str = "labels";

#[derive(Debug)]
pub struct Labels {
    script: Option<Script>,
}

impl Labels {
    pub fn of(script: &Script) -> Labels {
        Labels { script: (script.kind(LABELS) == "function").then(|| script.clone()) }
    }

    pub fn of_result(&self, connector: &str, action: &str, result: &Value) -> Option<String> {
        let script = self.script.as_ref()?;
        let input = json!({ "action": action, "result": result });
        match script.invoke_here(LABELS, &input, Arc::new(Local)) {
            Ok(Value::String(label)) => Some(label),
            Ok(_) => None,
            Err(error) => {
                tracing::warn!(connector, action, error = error.message, "the connector's labels did not run");
                None
            }
        }
    }
}
