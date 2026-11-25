mod asked;

use domain::mcp::text_result;
use serde_json::Value;

use crate::session::Ask;

pub(crate) const MOST: usize = 20;

const STOP_WORDS: [&str; 22] = [
    "the",
    "a",
    "an",
    "of",
    "on",
    "in",
    "to",
    "for",
    "with",
    "and",
    "or",
    "that",
    "this",
    "is",
    "at",
    "by",
    "from",
    "element",
    "elements",
    "containing",
    "contains",
    "named",
];

const ROLE_WORDS: [(&str, &[&str]); 21] = [
    ("button", &["button"]),
    ("btn", &["button"]),
    ("link", &["link"]),
    ("links", &["link"]),
    ("input", &["textbox", "searchbox", "combobox", "spinbutton", "checkbox", "radio", "slider"]),
    ("field", &["textbox", "searchbox", "combobox", "spinbutton"]),
    ("box", &["textbox", "searchbox", "combobox", "checkbox"]),
    ("textbox", &["textbox", "searchbox"]),
    ("textarea", &["textbox"]),
    ("bar", &["searchbox", "textbox", "combobox", "navigation", "toolbar", "menubar", "tablist"]),
    ("checkbox", &["checkbox"]),
    ("radio", &["radio"]),
    ("dropdown", &["combobox", "listbox", "button"]),
    ("select", &["combobox", "listbox"]),
    ("tab", &["tab"]),
    ("menu", &["menu", "menubar", "menuitem", "button"]),
    ("image", &["img", "image"]),
    ("icon", &["img", "image", "button"]),
    ("heading", &["heading"]),
    ("title", &["heading"]),
    ("upload", &["file"]),
];

#[derive(Debug)]
struct Scored<'a> {
    score: f64,
    candidate: &'a Value,
}

pub fn answer(query: &str, found: &Value, ask: Option<&Ask>) -> Value {
    let query = query.trim();
    if query.is_empty() {
        return text_result("Describe what to find, e.g. \"search bar\" or \"add to cart button\".", true);
    }
    let candidates = found["candidates"].as_array().cloned().unwrap_or_default();
    let ranked = rank(query, &candidates);
    let by_score: Vec<&Value> = ranked.iter().map(|scored| scored.candidate).collect();
    if let Some(chosen) = ask.and_then(|ask| asked::choose(ask, query, &candidates, &by_score)) {
        return listing(query, &chosen.picked, chosen.more.then_some(None));
    }
    let more = (by_score.len() > MOST).then_some(Some(by_score.len()));
    listing(query, &by_score, more)
}

fn listing(query: &str, picked: &[&Value], more: Option<Option<usize>>) -> Value {
    if picked.is_empty() {
        return text_result(
            format!(
                "No elements found matching \"{query}\". Try a different description, or read_page to see the page's structure (or a screenshot for visual content)."
            ),
            false,
        );
    }
    let lines: Vec<String> = picked.iter().take(MOST).map(|candidate| line(candidate)).collect();
    let mut text = format!(
        "Found {} matching element{}:\n\n{}",
        lines.len(),
        if lines.len() == 1 { "" } else { "s" },
        lines.join("\n")
    );
    match more {
        Some(Some(total)) => text.push_str(&format!(
            "\n\nMore than {MOST} elements match ({total} in all); use a more specific query to narrow them down."
        )),
        Some(None) => text
            .push_str(&format!("\n\nMore than {MOST} elements match; use a more specific query to narrow them down.")),
        None => {}
    }
    text_result(text, false)
}

fn rank<'a>(query: &str, candidates: &'a [Value]) -> Vec<Scored<'a>> {
    let lowered = query.to_lowercase();
    let words = words(&lowered);
    let (roles, terms): (Vec<&str>, Vec<&str>) =
        words.iter().map(String::as_str).partition(|word| ROLE_WORDS.iter().any(|(role_word, _)| role_word == word));
    let wanted_roles: Vec<&str> = roles
        .iter()
        .flat_map(|word| ROLE_WORDS.iter().filter(move |(role_word, _)| role_word == word))
        .flat_map(|(_, roles)| roles.iter().copied())
        .collect();
    let mut scored: Vec<Scored<'a>> = candidates
        .iter()
        .map(|candidate| Scored { score: score(candidate, &lowered, &terms, &roles, &wanted_roles), candidate })
        .filter(|scored| scored.score > 0.0)
        .collect();
    scored.sort_by(|a, b| b.score.total_cmp(&a.score));
    let best = scored.first().map_or(0.0, |scored| scored.score);
    let floor = (best * 0.45).max(1.0);
    scored.retain(|scored| scored.score >= floor);
    scored
}

fn score(candidate: &Value, phrase: &str, terms: &[&str], role_words: &[&str], roles: &[&str]) -> f64 {
    let field = |name: &str| candidate[name].as_str().unwrap_or_default().to_lowercase();
    let (name, text, role) = (field("name"), field("text"), field("role"));
    let attributes = attributes(candidate);
    let mut total = 0.0;
    let mut matched_terms = 0;
    for term in terms {
        let gained = term_score(term, &name, &text, &attributes);
        if gained > 0.0 {
            matched_terms += 1;
        }
        total += gained;
    }
    if !terms.is_empty() && matched_terms == 0 {
        return role_only(role_words, terms, roles, &role, &name, &attributes);
    }
    if !terms.is_empty() {
        total *= f64::from(matched_terms) / terms.len() as f64;
    }
    let phrase_terms = terms.join(" ");
    if phrase_terms.len() > 2 && (name.contains(&phrase_terms) || text.contains(&phrase_terms)) {
        total += 3.0;
    }
    if !phrase.is_empty() && name == phrase {
        total += 3.0;
    }
    total += role_bonus(roles, &role, &name, &attributes);
    if candidate["interactive"] == true {
        total += 0.4;
    }
    if candidate["visible"] == true {
        total += 0.3;
    }
    total
}

fn role_only(role_words: &[&str], terms: &[&str], roles: &[&str], role: &str, name: &str, attributes: &str) -> f64 {
    if role_words.is_empty() || terms.iter().any(|term| term.len() > 3) {
        return 0.0;
    }
    role_bonus(roles, role, name, attributes)
}

fn role_bonus(roles: &[&str], role: &str, name: &str, attributes: &str) -> f64 {
    if roles.is_empty() {
        return 0.0;
    }
    if roles.contains(&role) || (roles.contains(&"file") && attributes.contains("type=file")) {
        return 2.5;
    }
    if roles.contains(&"searchbox") && (name.contains("search") || attributes.contains("search")) && role == "textbox" {
        return 2.0;
    }
    0.0
}

fn term_score(term: &str, name: &str, text: &str, attributes: &str) -> f64 {
    if has_word(name, term) {
        3.0
    } else if name.contains(term) {
        2.0
    } else if has_word(text, term) {
        1.6
    } else if has_word(attributes, term) || attributes.contains(term) {
        1.2
    } else if text.contains(term) {
        1.0
    } else if term.len() >= 5 && (near(name, term) || near(attributes, term)) {
        0.8
    } else {
        0.0
    }
}

fn attributes(candidate: &Value) -> String {
    candidate["attrs"]
        .as_object()
        .map(|attrs| {
            attrs
                .iter()
                .map(|(key, value)| {
                    format!("{key}={}", value.as_str().map_or_else(|| value.to_string(), str::to_owned))
                })
                .collect::<Vec<_>>()
                .join(" ")
                .to_lowercase()
        })
        .unwrap_or_default()
}

fn words(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|word| !word.is_empty() && !STOP_WORDS.contains(word))
        .map(str::to_owned)
        .collect()
}

fn has_word(haystack: &str, word: &str) -> bool {
    haystack
        .split(|c: char| !c.is_alphanumeric())
        .any(|part| part == word || part.strip_suffix('s') == Some(word) || word.strip_suffix('s') == Some(part))
}

fn near(haystack: &str, term: &str) -> bool {
    let stem = &term[..term.char_indices().nth(term.chars().count() * 3 / 4).map_or(term.len(), |(at, _)| at)];
    haystack
        .split(|c: char| !c.is_alphanumeric())
        .any(|part| part.len() >= 4 && (part.starts_with(stem) || term.starts_with(part)))
}

pub(crate) fn line(candidate: &Value) -> String {
    let field = |name: &str| candidate[name].as_str().unwrap_or_default();
    let mut line = format!("- {}: {}", field("ref"), field("role"));
    if !field("name").is_empty() {
        line.push_str(&format!(" \"{}\"", clip(field("name"), 100)));
    }
    if let Some(attrs) = candidate["attrs"].as_object() {
        for (key, value) in
            attrs.iter().filter(|(key, _)| ["type", "placeholder", "href", "value"].contains(&key.as_str()))
        {
            let value = value.as_str().map_or_else(|| value.to_string(), str::to_owned);
            line.push_str(&format!(" {key}=\"{}\"", clip(&value, 80)));
        }
    }
    if candidate["visible"] == false {
        line.push_str(" (off-screen)");
    }
    if !field("context").is_empty() {
        line.push_str(&format!(" — in {}", field("context")));
    }
    line
}

fn clip(text: &str, most: usize) -> String {
    match text.char_indices().nth(most) {
        Some((at, _)) => format!("{}…", &text[..at]),
        None => text.to_owned(),
    }
}

#[cfg(test)]
mod tests;
