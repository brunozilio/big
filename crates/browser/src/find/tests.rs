use serde_json::json;

use super::*;

fn page() -> Value {
    json!({ "candidates": [
        { "ref": "ref_1", "role": "link", "name": "Home", "visible": true, "interactive": true, "attrs": { "href": "/" } },
        { "ref": "ref_2", "role": "textbox", "name": "Search products", "visible": true, "interactive": true, "attrs": { "placeholder": "Search products", "type": "search" }, "context": "header" },
        { "ref": "ref_3", "role": "button", "name": "Search", "visible": true, "interactive": true },
        { "ref": "ref_4", "role": "heading", "name": "Organic mango, 1 kg", "visible": true },
        { "ref": "ref_5", "role": "button", "name": "Add to cart", "visible": true, "interactive": true, "context": "Organic mango, 1 kg" },
        { "ref": "ref_6", "role": "button", "name": "Sign in", "visible": true, "interactive": true },
        { "ref": "ref_7", "role": "textbox", "name": "Email", "visible": false, "interactive": true, "attrs": { "type": "email" } },
        { "ref": "ref_8", "role": "button", "name": "Upload", "interactive": true, "attrs": { "type": "file" } }
    ]})
}

fn refs(query: &str) -> Vec<String> {
    let candidates = page()["candidates"].as_array().cloned().unwrap_or_default();
    rank(query, &candidates)
        .iter()
        .map(|scored| scored.candidate["ref"].as_str().unwrap_or_default().to_owned())
        .collect()
}

#[test]
fn purpose_and_text_queries_put_the_right_element_first() {
    assert_eq!(refs("search bar").first().map(String::as_str), Some("ref_2"));
    assert_eq!(refs("search button").first().map(String::as_str), Some("ref_3"));
    assert_eq!(refs("add to cart button").first().map(String::as_str), Some("ref_5"));
    assert_eq!(refs("organic mango product").first().map(String::as_str), Some("ref_4"));
    assert_eq!(refs("sign in button").first().map(String::as_str), Some("ref_6"));
    assert_eq!(refs("email field").first().map(String::as_str), Some("ref_7"));
    assert_eq!(refs("file upload").first().map(String::as_str), Some("ref_8"));
}

#[test]
fn nothing_matching_says_so_and_matches_list_refs_roles_and_names() {
    let none = answer("checkout wizard", &page(), None);
    assert!(none["content"][0]["text"].as_str().unwrap_or_default().starts_with("No elements found"));
    let found = answer("search", &page(), None);
    let text = found["content"][0]["text"].as_str().unwrap_or_default();
    assert!(text.starts_with("Found "), "{text}");
    assert!(text.contains("- ref_3: button \"Search\""), "{text}");
    assert!(text.contains("placeholder=\"Search products\""), "{text}");
    assert_eq!(answer(" ", &page(), None)["isError"], true);
}

#[test]
fn more_than_twenty_matches_are_cut_with_a_note() {
    let many: Vec<Value> = (0..30)
        .map(|n| json!({ "ref": format!("ref_{n}"), "role": "link", "name": format!("Product {n}"), "interactive": true }))
        .collect();
    let found = answer("product link", &json!({ "candidates": many }), None);
    let text = found["content"][0]["text"].as_str().unwrap_or_default();
    assert!(text.starts_with("Found 20 matching elements"), "{text}");
    assert!(text.contains("More than 20 elements match (30 in all)"), "{text}");
}
