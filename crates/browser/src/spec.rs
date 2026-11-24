use std::sync::OnceLock;

use serde_json::Value;

const DECLARED: &str = include_str!("tools.json");

pub fn server_name() -> String {
    format!("{}_browser", brand::COMMAND.replace('-', "_"))
}

pub fn tools() -> &'static [Value] {
    static TOOLS: OnceLock<Vec<Value>> = OnceLock::new();
    TOOLS.get_or_init(|| serde_json::from_str(&brand::text(DECLARED)).unwrap_or_default())
}

pub fn known(name: &str) -> bool {
    tools().iter().any(|tool| tool["name"] == name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_twenty_two_tools_are_declared_with_object_schemas_and_the_brand_filled() {
        let names: Vec<&str> = tools().iter().filter_map(|tool| tool["name"].as_str()).collect();
        assert_eq!(
            names,
            [
                "browser_batch",
                "computer",
                "file_upload",
                "find",
                "form_input",
                "get_page_text",
                "gif_creator",
                "javascript_tool",
                "list_connected_browsers",
                "navigate",
                "read_console_messages",
                "read_network_requests",
                "read_page",
                "resize_window",
                "select_browser",
                "shortcuts_execute",
                "shortcuts_list",
                "switch_browser",
                "tabs_close_mcp",
                "tabs_context_mcp",
                "tabs_create_mcp",
                "upload_image",
            ]
        );
        for tool in tools() {
            assert_eq!(tool["inputSchema"]["type"], "object", "{}", tool["name"]);
            assert!(!tool["description"].as_str().unwrap_or_default().contains("{{"), "{}", tool["name"]);
        }
        assert!(server_name().ends_with("_browser"));
    }
}
