use serde_json::Value;

pub const WEB_SEARCH: &str = "WebSearch";

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Hit {
    pub title: String,
    pub url: String,
    #[serde(default)]
    pub snippet: String,
}

pub fn put(native: Option<Value>, body: &mut Value) {
    let (Some(native), Some(tools)) = (native, body["tools"].as_array_mut()) else { return };
    let named = |tool: &Value| tool["name"] == WEB_SEARCH || tool["function"]["name"] == WEB_SEARCH;
    if let Some(tool) = tools.iter_mut().find(|tool| named(tool)) {
        let mut replaced = native;
        if let Some(cache) = tool.get("cache_control") {
            replaced["cache_control"] = cache.clone();
        }
        *tool = replaced;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_function_tool_is_replaced_by_the_providers_own() {
        let mut body = json!({ "tools": [
            { "name": "Read", "input_schema": {} },
            { "name": WEB_SEARCH, "input_schema": {}, "cache_control": { "type": "ephemeral" } },
        ]});
        put(Some(json!({ "type": "native_search", "name": "web_search" })), &mut body);
        assert_eq!(body["tools"][0]["name"], "Read");
        assert_eq!(
            body["tools"][1],
            json!({ "type": "native_search", "name": "web_search", "cache_control": { "type": "ephemeral" } })
        );

        let mut responses_body = json!({ "tools": [{ "type": "function", "name": WEB_SEARCH, "parameters": {} }] });
        put(Some(json!({ "type": "web_search" })), &mut responses_body);
        assert_eq!(responses_body["tools"], json!([{ "type": "web_search" }]));
    }

    #[test]
    fn without_a_native_search_the_tool_is_left_as_it_is() {
        let mut body = json!({ "tools": [{ "name": WEB_SEARCH }] });
        put(None, &mut body);
        assert_eq!(body["tools"][0]["name"], WEB_SEARCH);
    }
}
