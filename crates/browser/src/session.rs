use std::path::PathBuf;
use std::sync::Arc;

use domain::mcp::{LocalMcp, McpEndpoint, RpcError, text_result};
use serde_json::{Value, json};

use crate::hub::{BrowserInfo, Hub, Resolved, no_browser};
use crate::{Settings, batch, files, find, recording, spec};

pub type Ask = Arc<dyn Fn(&str, &str) -> Option<String> + Send + Sync>;

#[derive(Clone)]
pub struct Session {
    hub: Hub,
    key: String,
    settings: Settings,
    roots: Vec<PathBuf>,
    ask: Option<Ask>,
}

impl std::fmt::Debug for Session {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Session")
            .field("key", &self.key)
            .field("asks_a_model", &self.ask.is_some())
            .finish_non_exhaustive()
    }
}

impl Session {
    pub fn new(hub: Hub, key: impl Into<String>, settings: Settings, roots: Vec<PathBuf>) -> Self {
        Self { hub, key: key.into(), settings, roots, ask: None }
    }

    pub fn asking(mut self, ask: Option<Ask>) -> Self {
        self.ask = ask;
        self
    }

    pub fn local(self) -> LocalMcp {
        LocalMcp::new(spec::server_name(), Arc::new(self))
    }

    pub(crate) fn settings(&self) -> &Settings {
        &self.settings
    }

    fn call(&self, name: &str, input: &Value) -> Result<Value, RpcError> {
        if !spec::known(name) {
            return Err((-32602, format!("unknown tool: {name}")));
        }
        Ok(match name {
            "browser_batch" => batch::run(self, input),
            _ => self.dispatch(name, input, true),
        })
    }

    pub(crate) fn dispatch(&self, name: &str, input: &Value, standalone: bool) -> Value {
        match name {
            "list_connected_browsers" => self.listed(),
            "select_browser" => self.select(input),
            "switch_browser" => self.switch(),
            _ => match self.device() {
                Ok(device) => self.forward(&device, name, input, standalone),
                Err(message) => text_result(message, true),
            },
        }
    }

    fn device(&self) -> Result<String, String> {
        match self.hub.resolve(&self.key) {
            Resolved::Browser(device) => Ok(device),
            Resolved::NoneConnected => Err(no_browser()),
            Resolved::Several(browsers) => Err(several(&browsers)),
        }
    }

    fn forward(&self, device: &str, name: &str, input: &Value, standalone: bool) -> Value {
        let prepared = match name {
            "file_upload" => files::attach(input, &self.roots, self.settings.upload_limit),
            "gif_creator" if input["action"] == "export" => return recording::export(self, device, input),
            _ => Ok(input.clone()),
        };
        let input = match prepared {
            Ok(input) => input,
            Err(message) => return text_result(message, true),
        };
        let answer = match self.ask(device, name, &input, standalone) {
            Ok(answer) => answer,
            Err(message) => return text_result(message, true),
        };
        match name {
            "find" if answer["isError"] == true => answer,
            "find" => find::answer(input["query"].as_str().unwrap_or_default(), &answer, self.ask.as_ref()),
            "computer" if input["save_to_disk"] == true => files::save_images(answer, &self.settings.files),
            _ => well_formed(answer),
        }
    }

    pub(crate) fn ask(&self, device: &str, tool: &str, input: &Value, standalone: bool) -> Result<Value, String> {
        let sites = approved_sites(tool, input);
        let message = json!({ "type": "call", "session": self.key, "tool": tool, "input": input, "standalone": standalone, "sites": sites });
        let mut answer = self.hub.call(device, message, self.settings.call)?;
        if let Some(tabs) = answer.as_object_mut().and_then(|fields| fields.remove("tabs")) {
            remember_tabs(&tabs);
        }
        Ok(answer)
    }

    fn listed(&self) -> Value {
        let chosen = self.hub.chosen(&self.key);
        let browsers = self.hub.browsers();
        if browsers.is_empty() {
            return text_result(no_browser(), true);
        }
        let only = browsers.len() == 1;
        let listed: Vec<Value> = browsers
            .iter()
            .map(|browser| {
                let in_use = chosen.as_deref() == Some(browser.device_id.as_str()) || (only && chosen.is_none());
                json!({
                    "deviceId": browser.device_id,
                    "name": browser.name,
                    "platform": browser.platform,
                    "isLocal": same_os(&browser.platform),
                    "onThisComputer": true,
                    "inUse": in_use,
                })
            })
            .collect();
        text_result(serde_json::to_string_pretty(&listed).unwrap_or_default(), false)
    }

    fn select(&self, input: &Value) -> Value {
        let device = input["deviceId"].as_str().unwrap_or_default().trim();
        if device.is_empty() {
            return text_result("Give the deviceId of a browser from list_connected_browsers.", true);
        }
        if !self.hub.choose(&self.key, device) {
            return text_result(
                format!(
                    "No connected browser has deviceId \"{device}\". Call list_connected_browsers for the current list."
                ),
                true,
            );
        }
        let name =
            self.hub.browsers().into_iter().find(|browser| browser.device_id == device).map(|browser| browser.name);
        text_result(
            format!(
                "Selected browser \"{}\" ({device}). Browser tools in this session now act in it; call tabs_context_mcp to see its tabs.",
                name.unwrap_or_default()
            ),
            false,
        )
    }

    fn switch(&self) -> Value {
        match self.hub.pair(&self.key, self.settings.pairing) {
            Ok(browser) => text_result(
                format!(
                    "Connected to browser \"{}\" ({}). Browser tools in this session now act in it; call tabs_context_mcp to see its tabs.",
                    browser.name, browser.device_id
                ),
                false,
            ),
            Err(message) => text_result(message, true),
        }
    }
}

impl McpEndpoint for Session {
    fn handle(&self, message: &Value) -> Option<Value> {
        let mut reply = domain::mcp::respond(
            message,
            &spec::server_name(),
            || spec::tools().to_vec(),
            |name, input| self.call(name, input),
        )?;
        if message["method"] == "initialize" {
            reply["result"]["instructions"] = json!(spec::instructions());
        }
        Some(reply)
    }
}

fn approved_sites(tool: &str, input: &Value) -> Vec<String> {
    let mut sites =
        domain::permission::allowed::global().map(|allowed| allowed.sites()).unwrap_or_else(|| vec!["*".to_owned()]);
    sites.extend(domain::browser_sites::sites(tool, input).into_iter().flatten());
    sites
}

fn remember_tabs(tabs: &Value) {
    for tab in tabs.as_array().map(Vec::as_slice).unwrap_or_default() {
        if let Some(id) = tab["id"].as_i64() {
            domain::browser_sites::record(id, tab["url"].as_str().unwrap_or_default());
        }
    }
}

fn well_formed(answer: Value) -> Value {
    if answer["content"].is_array() {
        return answer;
    }
    let text = answer.as_str().map_or_else(|| answer.to_string(), str::to_owned);
    text_result(text, false)
}

fn several(browsers: &[BrowserInfo]) -> String {
    let listed: Vec<String> = browsers
        .iter()
        .map(|browser| format!("- {} ({}, {})", browser.name, browser.device_id, browser.platform))
        .collect();
    format!(
        "Several browsers are connected and none is selected for this session:\n{}\nAsk the user which one to use with AskUserQuestion (call list_connected_browsers for the details), then call select_browser with the chosen deviceId, or switch_browser to let the user pick inside Chrome.",
        listed.join("\n")
    )
}

fn same_os(platform: &str) -> bool {
    let platform = platform.to_ascii_lowercase();
    match std::env::consts::OS {
        "macos" => platform.contains("mac"),
        "windows" => platform.contains("win"),
        "linux" => platform.contains("linux") || platform.contains("cros"),
        other => platform.contains(other),
    }
}

#[cfg(test)]
mod tests;
