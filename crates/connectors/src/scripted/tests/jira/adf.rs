use serde_json::{Value, json};

use super::probe;

fn adf_to_markdown(document: &Value) -> String {
    let found = probe("connectors/jira/adf.ts", "return m.adfToMarkdown(input);", document.clone());
    found.as_str().unwrap_or_default().to_owned()
}

fn document(blocks: Value) -> Value {
    json!({"type": "doc", "version": 1, "content": blocks})
}

fn text(text: &str) -> Value {
    json!({"type": "paragraph", "content": [{"type": "text", "text": text}]})
}

#[test]
fn headings_paragraphs_and_hard_breaks_read_as_markdown() {
    let blocks = json!([
        {"type": "heading", "attrs": {"level": 2}, "content": [{"type": "text", "text": "Critérios"}]},
        {"type": "paragraph", "content": [{"type": "text", "text": "one"}, {"type": "hardBreak"}, {"type": "text", "text": "two"}]},
        text("three")
    ]);
    assert_eq!(adf_to_markdown(&document(blocks)), "## Critérios\n\none\ntwo\n\nthree");
}

#[test]
fn lists_and_checklists_read_as_markdown_lists() {
    let item = |text: &str| json!({"type": "listItem", "content": [{"type": "paragraph", "content": [{"type": "text", "text": text}]}]});
    let blocks = json!([
        {"type": "bulletList", "content": [item("a"), item("b")]},
        {"type": "orderedList", "content": [item("c"), item("d")]},
        {"type": "taskList", "attrs": {"localId": "l"}, "content": [
            {"type": "taskItem", "attrs": {"localId": "t1", "state": "TODO"}, "content": [{"type": "text", "text": "escrever testes"}]},
            {"type": "taskItem", "attrs": {"localId": "t2", "state": "DONE"}, "content": [{"type": "text", "text": "revisar"}]}
        ]}
    ]);
    assert_eq!(adf_to_markdown(&document(blocks)), "- a\n- b\n\n1. c\n2. d\n\n- [ ] escrever testes\n- [x] revisar");
}

#[test]
fn code_quotes_rules_and_marks_read_as_markdown() {
    let blocks = json!([
        {"type": "codeBlock", "attrs": {"language": "rust"}, "content": [{"type": "text", "text": "fn main() {}"}]},
        {"type": "blockquote", "content": [text("citado")]},
        {"type": "rule"},
        {"type": "paragraph", "content": [
            {"type": "text", "text": "cargo test", "marks": [{"type": "code"}]},
            {"type": "text", "text": " "},
            {"type": "text", "text": "PR", "marks": [{"type": "link", "attrs": {"href": "https://g/1"}}, {"type": "strong"}]},
            {"type": "text", "text": " "},
            {"type": "text", "text": "ok", "marks": [{"type": "em"}]}
        ]}
    ]);
    assert_eq!(
        adf_to_markdown(&document(blocks)),
        "```rust\nfn main() {}\n```\n\n> citado\n\n---\n\n`cargo test` **[PR](https://g/1)** *ok*"
    );
}

#[test]
fn mentions_and_unknown_nodes_read_as_text() {
    let document = json!({"type": "doc", "version": 1, "content": [
        {"type": "paragraph", "content": [{"type": "mention", "attrs": {"id": "1", "text": "@Ana"}}, {"type": "text", "text": " veja"}]},
        {"type": "panel", "attrs": {"panelType": "info"}, "content": [{"type": "paragraph", "content": [{"type": "text", "text": "dentro"}]}]}
    ]});
    assert_eq!(adf_to_markdown(&document), "@Ana veja\n\ndentro");
    assert_eq!(adf_to_markdown(&Value::Null), "");
    assert_eq!(adf_to_markdown(&json!("já em markdown")), "já em markdown");
}
