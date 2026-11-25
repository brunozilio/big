use serde_json::{Value, json};

use crate::session::Session;
use crate::spec;

const NEEDS_TAB: [&str; 15] = [
    "computer",
    "file_upload",
    "find",
    "form_input",
    "get_page_text",
    "gif_creator",
    "javascript_tool",
    "navigate",
    "network_intercept",
    "read_console_messages",
    "read_network_requests",
    "read_page",
    "resize_window",
    "shortcuts_execute",
    "upload_image",
];

pub fn run(session: &Session, input: &Value) -> Value {
    let Some(actions) = input["actions"].as_array().filter(|actions| !actions.is_empty()) else {
        return failed(vec![], "`actions` must be a non-empty list of {name, input} items.");
    };
    let total = actions.len();
    let mut content = Vec::new();
    for (index, action) in actions.iter().enumerate() {
        let position = format!("[{}/{total}]", index + 1);
        let name = short_name(action["name"].as_str().unwrap_or_default());
        let input = action.get("input").cloned().unwrap_or_else(|| json!({}));
        if let Err(problem) = checked(name, &input) {
            content.push(text(format!("{position} {name}: {problem}")));
            return failed(content, &stopped(index, total));
        }
        let answer = session.dispatch(name, &input, false);
        let is_error = answer["isError"] == true;
        content.extend(labelled(&position, name, &answer));
        if is_error {
            return failed(content, &stopped(index, total));
        }
    }
    json!({ "content": content, "isError": false })
}

pub fn short_name(name: &str) -> &str {
    let server = spec::server_name();
    name.strip_prefix("mcp__")
        .and_then(|rest| rest.strip_prefix(server.as_str()))
        .and_then(|rest| rest.strip_prefix("__"))
        .unwrap_or(name)
}

fn checked(name: &str, input: &Value) -> Result<(), String> {
    if name == "browser_batch" {
        return Err("browser_batch cannot be nested.".to_owned());
    }
    if !spec::known(name) {
        return Err(format!("Unknown browser tool \"{name}\"."));
    }
    if !input.is_object() {
        return Err("`input` must be an object with that tool's parameters.".to_owned());
    }
    if NEEDS_TAB.contains(&name) && !input["tabId"].is_number() {
        return Err(format!("{name} inside browser_batch requires an explicit tabId."));
    }
    Ok(())
}

fn labelled(position: &str, name: &str, answer: &Value) -> Vec<Value> {
    let parts = answer["content"].as_array().cloned().unwrap_or_default();
    let mut labelled_first = false;
    let mut out: Vec<Value> = parts
        .into_iter()
        .map(|mut part| {
            if !labelled_first && part["type"] == "text" {
                labelled_first = true;
                let body = part["text"].as_str().unwrap_or_default();
                part["text"] = json!(format!("{position} {name}: {body}"));
            }
            part
        })
        .collect();
    if !labelled_first {
        out.insert(0, text(format!("{position} {name}: done")));
    }
    out
}

fn stopped(index: usize, total: usize) -> String {
    let left = total - index - 1;
    if left == 0 {
        format!("Batch stopped at action {} of {total}.", index + 1)
    } else {
        format!("Batch stopped at action {} of {total}; the remaining {left} action(s) did not run.", index + 1)
    }
}

fn failed(mut content: Vec<Value>, message: &str) -> Value {
    content.push(text(message.to_owned()));
    json!({ "content": content, "isError": true })
}

fn text(text: String) -> Value {
    json!({ "type": "text", "text": text })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_with_the_server_prefix_are_shortened() {
        let full = format!("mcp__{}__computer", spec::server_name());
        assert_eq!(short_name(&full), "computer");
        assert_eq!(short_name("navigate"), "navigate");
    }

    #[test]
    fn nesting_unknown_tools_and_missing_tabs_are_refused_before_running() {
        assert!(checked("browser_batch", &json!({})).is_err());
        assert!(checked("nope", &json!({})).is_err());
        assert!(checked("navigate", &json!({ "url": "a.com" })).is_err());
        assert!(checked("navigate", &json!({ "url": "a.com", "tabId": 1 })).is_ok());
        assert!(checked("tabs_create_mcp", &json!({})).is_ok());
        assert!(checked("computer", &json!("x")).is_err());
    }

    #[test]
    fn the_first_text_of_each_answer_is_labelled_and_images_kept_in_place() {
        let answer = json!({ "content": [
            { "type": "text", "text": "Clicked at (1, 2)" },
            { "type": "image", "data": "AA", "mimeType": "image/jpeg" }
        ]});
        let parts = labelled("[1/2]", "computer", &answer);
        assert_eq!(parts[0]["text"], "[1/2] computer: Clicked at (1, 2)");
        assert_eq!(parts[1]["type"], "image");
        let bare = labelled("[2/2]", "computer", &json!({ "content": [{ "type": "image", "data": "AA" }] }));
        assert_eq!(bare[0]["text"], "[2/2] computer: done");
        assert_eq!(stopped(0, 3), "Batch stopped at action 1 of 3; the remaining 2 action(s) did not run.");
    }
}
