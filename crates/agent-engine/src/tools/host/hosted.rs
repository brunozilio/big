use serde_json::Value;

use super::{Reach, text};

pub const SPACE: &str = "hosted";

pub fn call(reach: &Reach<'_>, name: &str, arguments: &Value) -> Result<Value, String> {
    match name {
        "call" => {
            let input = arguments.get("input").unwrap_or(&Value::Null);
            let client = reach.workspace.credentials.client;
            crate::tools::hosted::call(client, text(arguments, "name")?, input, reach.workspace)
        }
        other => Err(format!("ctx.{SPACE}.{other} is not a function")),
    }
}
