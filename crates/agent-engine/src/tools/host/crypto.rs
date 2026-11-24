use serde_json::Value;

use super::Reach;

pub const SPACE: &str = "crypto";

pub fn call(_reach: &Reach<'_>, name: &str, arguments: &Value) -> Result<Value, String> {
    platform::crypto::call(name, arguments).map_err(|error| format!("ctx.{error}"))
}
