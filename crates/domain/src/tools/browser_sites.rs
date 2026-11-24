use std::collections::HashMap;
use std::sync::{LazyLock, PoisonError, RwLock};

use serde_json::Value;

use crate::permission::Effect;

const ON_A_PAGE: [&str; 12] = [
    "computer",
    "file_upload",
    "find",
    "form_input",
    "get_page_text",
    "javascript_tool",
    "navigate",
    "read_console_messages",
    "read_network_requests",
    "read_page",
    "shortcuts_execute",
    "upload_image",
];

static TABS: LazyLock<RwLock<HashMap<i64, String>>> = LazyLock::new(RwLock::default);

pub fn record(tab: i64, url: &str) {
    let mut tabs = TABS.write().unwrap_or_else(PoisonError::into_inner);
    match crate::site::site(url).filter(|_| url.starts_with("http")) {
        Some(host) => tabs.insert(tab, host),
        None => tabs.remove(&tab),
    };
}

pub fn site_of_tab(tab: i64) -> Option<String> {
    TABS.read().unwrap_or_else(PoisonError::into_inner).get(&tab).cloned()
}

fn tab_of(input: &Value) -> Option<i64> {
    input["tabId"].as_i64().or_else(|| input["tabId"].as_str().and_then(|text| text.trim().parse().ok()))
}

fn short(name: &str) -> &str {
    name.rsplit("__").next().unwrap_or(name)
}

pub fn sites(tool: &str, input: &Value) -> Vec<Option<String>> {
    let tool = short(tool);
    if tool == "browser_batch" {
        let actions = input["actions"].as_array().map(Vec::as_slice).unwrap_or_default();
        let empty = Value::Null;
        return actions
            .iter()
            .flat_map(|action| sites(action["name"].as_str().unwrap_or_default(), action.get("input").unwrap_or(&empty)))
            .collect();
    }
    if !ON_A_PAGE.contains(&tool) {
        return Vec::new();
    }
    let url = input["url"].as_str().map(str::trim).unwrap_or_default();
    if tool == "navigate" && !url.is_empty() && url != "back" && url != "forward" {
        let written = if url.contains("://") { url.to_owned() } else { format!("https://{url}") };
        return vec![crate::site::site(&written)];
    }
    vec![tab_of(input).and_then(site_of_tab)]
}

pub fn effects(server: &str, tool: &str, input: &Value) -> Vec<Effect> {
    let mut effects = vec![Effect::Remote { server: server.to_owned() }];
    let mut seen = Vec::new();
    for host in sites(tool, input) {
        if !seen.contains(&host) {
            seen.push(host.clone());
            effects.push(Effect::Fetch { host });
        }
    }
    effects
}

pub fn subject(tool: &str, input: &Value) -> Option<String> {
    let sites: Vec<String> = sites(tool, input).into_iter().map(|host| host.unwrap_or_else(|| "?".to_owned())).collect();
    (!sites.is_empty()).then(|| sites.join(", "))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn a_page_tool_names_the_site_of_its_tab_and_navigate_the_site_it_opens() {
        record(7, "https://www.gov.br/pt-br");
        assert_eq!(sites("mcp__x_browser__read_page", &json!({ "tabId": 7 })), [Some("www.gov.br".to_owned())]);
        assert_eq!(sites("navigate", &json!({ "url": "docs.rs/serde", "tabId": 7 })), [Some("docs.rs".to_owned())]);
        assert_eq!(sites("navigate", &json!({ "url": "back", "tabId": 7 })), [Some("www.gov.br".to_owned())]);
        assert_eq!(sites("computer", &json!({ "tabId": 99 })), [None]);
        assert!(sites("tabs_context_mcp", &json!({})).is_empty());
        let batch = json!({ "actions": [{ "name": "navigate", "input": { "url": "https://a.dev" } }, { "name": "find", "input": { "tabId": 7 } }] });
        assert_eq!(sites("browser_batch", &batch), [Some("a.dev".to_owned()), Some("www.gov.br".to_owned())]);
        record(7, "chrome://newtab");
        assert_eq!(site_of_tab(7), None);
    }
}
