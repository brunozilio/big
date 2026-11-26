use serde_json::{Value, json};

use crate::Settings;
use crate::hub::Hub;

#[derive(Debug)]
pub struct Eval<'a> {
    pub session: &'a str,
    pub tab: i64,
    pub text: &'a str,
    pub frame: Option<&'a Value>,
    pub sites: &'a [String],
}

pub fn run(hub: &Hub, device: &str, settings: &Settings, eval: &Eval) -> Result<Value, String> {
    let input = json!({ "action": "javascript_exec", "tabId": eval.tab, "text": eval.text, "frame": eval.frame });
    let message = json!({
        "type": "call",
        "session": eval.session,
        "tool": "javascript_tool",
        "input": input,
        "standalone": true,
        "sites": approved(eval.sites),
    });
    let answer = hub.call(device, message, settings.call)?;
    if answer["isError"] == true {
        return Err(text_of(&answer));
    }
    if answer["tooLarge"] == true {
        return Err("The result is larger than a connector can receive. Return only the fields you need.".to_owned());
    }
    Ok(answer["json"].clone())
}

fn approved(declared: &[String]) -> Vec<String> {
    let mut sites = declared.to_vec();
    sites.extend(domain::permission::allowed::global().map(|allowed| allowed.sites()).unwrap_or_default());
    sites
}

fn text_of(answer: &Value) -> String {
    answer["content"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|part| part["text"].as_str())
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_declared_sites_come_first_and_keep_the_approved_ones() {
        let declared = vec!["example.com".to_owned()];
        let sites = approved(&declared);
        assert_eq!(sites.first().map(String::as_str), Some("example.com"));
    }

    #[test]
    fn the_error_text_is_the_tools_text() {
        let answer = json!({ "isError": true, "content": [{ "type": "text", "text": "No frame" }] });
        assert_eq!(text_of(&answer), "No frame");
    }
}
