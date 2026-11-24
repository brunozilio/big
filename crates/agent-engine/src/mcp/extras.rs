use std::sync::Arc;

use serde_json::{Value, json};

use super::McpServer;
use crate::tools::Tool;

pub fn resource_tools(servers: &[Arc<McpServer>]) -> Vec<Box<dyn Tool>> {
    let names: Vec<&str> =
        servers.iter().filter(|server| server.offers("resources")).map(|server| server.name()).collect();
    if names.is_empty() {
        return Vec::new();
    }
    [("ListMcpResources", json!({ "servers": names })), ("ReadMcpResource", Value::Null)]
        .into_iter()
        .filter_map(|(name, data)| crate::tools::conditional::tool(name, &data))
        .map(|tool| Box::new(tool) as Box<dyn Tool>)
        .collect()
}

pub fn expand_prompt(servers: &[Arc<McpServer>], typed: &str, stop: &platform::Stop) -> Option<Result<String, String>> {
    let rest = typed.trim().strip_prefix("/mcp__")?;
    let (name, args) = rest.split_once(char::is_whitespace).unwrap_or((rest, ""));
    let (server_name, prompt) = name.split_once("__")?;
    let server = servers.iter().find(|server| server.name() == server_name && server.offers("prompts"))?;
    Some(prompt_text(server, prompt, args.trim(), stop))
}

fn prompt_text(server: &McpServer, prompt: &str, args: &str, stop: &platform::Stop) -> Result<String, String> {
    let listed = server.ask("prompts/list", json!({}), stop)?;
    let declared = listed["prompts"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|found| found["name"] == prompt)
        .ok_or_else(|| format!("{} has no prompt {prompt}", server.name()))?;
    let names: Vec<&str> =
        declared["arguments"].as_array().into_iter().flatten().filter_map(|arg| arg["name"].as_str()).collect();
    let mut words = args.split_whitespace().collect::<Vec<_>>();
    let mut arguments = serde_json::Map::new();
    for (at, name) in names.iter().enumerate() {
        if words.is_empty() {
            break;
        }
        let value =
            if at + 1 == names.len() { std::mem::take(&mut words).join(" ") } else { words.remove(0).to_owned() };
        arguments.insert((*name).to_owned(), Value::String(value));
    }
    let got = server.ask("prompts/get", json!({ "name": prompt, "arguments": arguments }), stop)?;
    let text: Vec<&str> = got["messages"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|message| message["content"]["text"].as_str())
        .collect();
    if text.is_empty() {
        return Err(format!("{}'s prompt {prompt} came back empty", server.name()));
    }
    Ok(text.join("\n\n"))
}

#[cfg(test)]
#[path = "extras_tests.rs"]
mod tests;
