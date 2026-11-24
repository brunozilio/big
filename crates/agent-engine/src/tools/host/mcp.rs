use std::sync::Arc;

use serde_json::{Value, json};

use super::{Reach, text};
use crate::mcp::McpServer;

pub const SPACE: &str = "mcp";

pub fn call(reach: &Reach<'_>, name: &str, arguments: &Value) -> Result<Value, String> {
    match name {
        "resources" => resources(reach, arguments),
        "read" => read(reach, arguments),
        other => Err(format!("ctx.{SPACE}.{other} is not a function")),
    }
}

fn with_resources<'a>(reach: &'a Reach<'_>, name: &str) -> Result<Vec<&'a Arc<McpServer>>, String> {
    let offering: Vec<&Arc<McpServer>> =
        reach.workspace.services.mcp.iter().filter(|server| server.offers("resources")).collect();
    let named: Vec<&Arc<McpServer>> = offering.iter().copied().filter(|server| server.name() == name).collect();
    if named.is_empty() {
        let names: Vec<&str> = offering.iter().map(|server| server.name()).collect();
        return Err(format!("No MCP server {name} with resources; these have some: {}", names.join(", ")));
    }
    Ok(named)
}

fn resources(reach: &Reach<'_>, arguments: &Value) -> Result<Value, String> {
    let servers = with_resources(reach, text(arguments, "server")?)?;
    let listed =
        servers.into_iter().map(|server| match server.ask("resources/list", json!({}), &reach.workspace.stop) {
            Ok(listed) => json!({ "server": server.name(), "resources": listed["resources"] }),
            Err(error) => json!({ "server": server.name(), "error": error.to_string() }),
        });
    Ok(Value::Array(listed.collect()))
}

fn read(reach: &Reach<'_>, arguments: &Value) -> Result<Value, String> {
    let servers = with_resources(reach, text(arguments, "server")?)?;
    let uri = text(arguments, "uri")?;
    let server = servers.first().ok_or("no server")?;
    server.ask("resources/read", json!({ "uri": uri }), &reach.workspace.stop).map_err(|error| error.to_string())
}
