use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use serde_json::{Value, json};

use crate::ProviderError;

pub const WRAPPER: &str = "@provider";

pub fn wrapper(entry: &str) -> String {
    let quoted = serde_json::to_string(entry).unwrap_or_default();
    format!(
        r#"import * as provider from {quoted};
const found = (name) => provider[name];
const classed = (error) => {{
  const message = String(error?.message ?? error);
  try {{ const parsed = JSON.parse(message); if (parsed && parsed.kind) return parsed; }} catch (_) {{}}
  return {{ kind: "invalid", message }};
}};
export function exported() {{
  return Object.keys(provider).filter((name) => typeof provider[name] === "function");
}}
export function invoke(name, input, ctx) {{
  const target = found(name);
  try {{
    if (typeof target !== "function") return {{ ok: target === undefined ? null : target }};
    const value = target(input, ctx);
    return {{ ok: value === undefined ? null : value }};
  }} catch (error) {{
    return {{ error: classed(error) }};
  }}
}}
"#
    )
}

pub fn failed(kind: &str, message: &str, port: Option<u16>) -> String {
    json!({ "kind": kind, "message": message, "port": port }).to_string()
}

pub fn encoded(error: &ProviderError) -> String {
    match error {
        ProviderError::Unauthorized(why) => failed("unauthorized", why, None),
        ProviderError::Invalid(why) => failed("invalid", why, None),
        ProviderError::PortInUse(port) => failed("port", &error.to_string(), Some(*port)),
    }
}

pub fn error_of(value: &Value) -> ProviderError {
    let message = value["message"].as_str().unwrap_or_default().to_owned();
    match value["kind"].as_str() {
        Some("unauthorized") => ProviderError::Unauthorized(message),
        Some("port") => value["port"]
            .as_u64()
            .and_then(|port| u16::try_from(port).ok())
            .map_or(ProviderError::Invalid(message), ProviderError::PortInUse),
        _ => ProviderError::Invalid(message),
    }
}

#[derive(Debug, Clone)]
pub struct Script {
    pub modules: Arc<Vec<(String, String)>>,
    pub data: Value,
}

impl Script {
    pub fn exported(&self, host: Arc<dyn script::Host>) -> Vec<String> {
        self.run("exported", &[], host).ok().and_then(|names| serde_json::from_value(names).ok()).unwrap_or_default()
    }

    pub fn invoke(&self, name: &str, input: Value, host: Arc<dyn script::Host>) -> Result<Value, ProviderError> {
        let answer = self.run("invoke", &[json!(name), input], host).map_err(ProviderError::Invalid)?;
        if let Some(error) = answer.get("error") {
            return Err(error_of(error));
        }
        Ok(answer.get("ok").cloned().unwrap_or(Value::Null))
    }

    fn run(&self, export: &str, arguments: &[Value], host: Arc<dyn script::Host>) -> Result<Value, String> {
        let stop = Arc::new(AtomicBool::new(false));
        let limit = script::Limit::Busy(script::MOST_BUSY);
        script::run_modules_in(&self.modules, WRAPPER, export, arguments, &self.data, host, stop, limit)
    }
}
