use serde_json::Value;

use super::MOST;
use crate::session::Ask;

const MOST_LISTED: usize = 800;
const MOST_CHARS: usize = 60_000;

const SYSTEM: &str = "You find elements on a web page from a description. You get the description and a list of the page's elements, one per line: ref | role | name | text | attributes | where it sits | whether it is on screen. Pick the elements that match the description by purpose or by text, best match first, at most 20. Prefer elements someone would act on (a search box over a label that says Search) unless the description asks for text. Leave out anything that does not really match; an empty list is a valid answer. Answer with JSON only, no prose: {\"refs\": [\"ref_12\", \"ref_40\"], \"more\": false}, where more is true when over 20 elements match.";

#[derive(Debug)]
pub struct Chosen<'a> {
    pub picked: Vec<&'a Value>,
    pub more: bool,
}

pub fn choose<'a>(ask: &Ask, query: &str, candidates: &'a [Value], by_score: &[&'a Value]) -> Option<Chosen<'a>> {
    if candidates.is_empty() {
        return Some(Chosen { picked: Vec::new(), more: false });
    }
    let listed = listed(candidates, by_score);
    let prompt = format!("Description: {query}\n\nElements:\n{listed}");
    let reply = ask(SYSTEM, &prompt)?;
    parse(&reply, candidates)
}

fn listed(candidates: &[Value], by_score: &[&Value]) -> String {
    let rest = candidates.iter().filter(|candidate| !by_score.iter().any(|kept| std::ptr::eq(*kept, *candidate)));
    let mut text = String::new();
    for candidate in by_score.iter().copied().chain(rest).take(MOST_LISTED) {
        let row = row(candidate);
        if text.len() + row.len() > MOST_CHARS {
            break;
        }
        text.push_str(&row);
        text.push('\n');
    }
    text
}

fn row(candidate: &Value) -> String {
    let field = |name: &str| candidate[name].as_str().unwrap_or_default().replace(['\n', '|'], " ");
    let attributes = candidate["attrs"]
        .as_object()
        .map(|attrs| {
            attrs
                .iter()
                .map(|(key, value)| {
                    format!("{key}={}", value.as_str().map_or_else(|| value.to_string(), str::to_owned))
                })
                .collect::<Vec<_>>()
                .join(" ")
        })
        .unwrap_or_default();
    let text = field("text");
    let text = if text == field("name") { String::new() } else { text };
    let shown = if candidate["visible"] == true { "on screen" } else { "off screen" };
    format!(
        "{} | {} | {} | {} | {} | {} | {shown}",
        field("ref"),
        field("role"),
        field("name"),
        text,
        attributes.replace('|', " "),
        field("context")
    )
}

fn parse<'a>(reply: &str, candidates: &'a [Value]) -> Option<Chosen<'a>> {
    let start = reply.find('{')?;
    let end = reply.rfind('}')?;
    let parsed: Value = serde_json::from_str(reply.get(start..=end)?).ok()?;
    let refs = parsed["refs"].as_array()?;
    let mut picked: Vec<&Value> = Vec::new();
    for wanted in refs.iter().filter_map(Value::as_str) {
        let found = candidates.iter().find(|candidate| candidate["ref"] == wanted);
        if let Some(found) = found.filter(|found| !picked.iter().any(|kept| std::ptr::eq(*kept, *found))) {
            picked.push(found);
        }
    }
    let more = parsed["more"] == true || picked.len() > MOST;
    picked.truncate(MOST);
    Some(Chosen { picked, more })
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use serde_json::json;

    use super::*;

    fn page() -> Vec<Value> {
        vec![
            json!({ "ref": "ref_1", "role": "link", "name": "Home", "visible": true }),
            json!({ "ref": "ref_2", "role": "textbox", "name": "", "attrs": { "placeholder": "Buscar produtos" }, "visible": true, "context": "banner" }),
            json!({ "ref": "ref_3", "role": "button", "name": "Entrar", "visible": false }),
        ]
    }

    #[test]
    fn the_model_sees_every_element_and_its_refs_are_kept_in_its_order() {
        let seen = Arc::new(Mutex::new(String::new()));
        let heard = Arc::clone(&seen);
        let ask: Ask = Arc::new(move |_system, prompt| {
            prompt.clone_into(&mut heard.lock().unwrap_or_else(std::sync::PoisonError::into_inner));
            Some("Here: {\"refs\": [\"ref_3\", \"ref_9\", \"ref_2\", \"ref_3\"], \"more\": false}".to_owned())
        });
        let candidates = page();
        let chosen = choose(&ask, "login button", &candidates, &[]).unwrap_or_else(|| unreachable!());
        let refs: Vec<&str> = chosen.picked.iter().filter_map(|candidate| candidate["ref"].as_str()).collect();
        assert_eq!(refs, ["ref_3", "ref_2"]);
        assert!(!chosen.more);
        let prompt = seen.lock().unwrap_or_else(std::sync::PoisonError::into_inner).clone();
        assert!(prompt.starts_with("Description: login button"), "{prompt}");
        assert!(
            prompt.contains("ref_2 | textbox |  |  | placeholder=Buscar produtos | banner | on screen"),
            "{prompt}"
        );
        assert!(prompt.contains("ref_3 | button | Entrar |  |  |  | off screen"), "{prompt}");
    }

    #[test]
    fn a_failed_or_unreadable_answer_falls_back() {
        let silent: Ask = Arc::new(|_, _| None);
        assert!(choose(&silent, "x", &page(), &[]).is_none());
        let rambling: Ask = Arc::new(|_, _| Some("I think it is the login button".to_owned()));
        assert!(choose(&rambling, "x", &page(), &[]).is_none());
    }

    #[test]
    fn the_best_scored_elements_are_listed_first_and_each_only_once() {
        let candidates = page();
        let listed = listed(&candidates, &[&candidates[2]]);
        let refs: Vec<&str> = listed.lines().filter_map(|line| line.split(" | ").next()).collect();
        assert_eq!(refs, ["ref_3", "ref_1", "ref_2"]);
    }
}
