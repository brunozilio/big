use super::*;

fn content(markdown: &str) -> Value {
    markdown_to_adf(markdown)["content"].clone()
}

#[test]
fn an_empty_text_is_an_empty_document() {
    assert_eq!(markdown_to_adf(""), json!({"type": "doc", "version": 1, "content": []}));
}

#[test]
fn headings_keep_their_level() {
    assert_eq!(
        content("## Critérios\n###### Fim\n####### not a heading"),
        json!([
            {"type": "heading", "attrs": {"level": 2}, "content": [{"type": "text", "text": "Critérios"}]},
            {"type": "heading", "attrs": {"level": 6}, "content": [{"type": "text", "text": "Fim"}]},
            {"type": "paragraph", "content": [{"type": "text", "text": "####### not a heading"}]}
        ])
    );
}

#[test]
fn lines_of_one_paragraph_are_joined_by_hard_breaks_and_blank_lines_split_paragraphs() {
    assert_eq!(
        content("one\ntwo\n\nthree"),
        json!([
            {"type": "paragraph", "content": [{"type": "text", "text": "one"}, {"type": "hardBreak"}, {"type": "text", "text": "two"}]},
            {"type": "paragraph", "content": [{"type": "text", "text": "three"}]}
        ])
    );
}

#[test]
fn bullet_and_ordered_lists_hold_paragraph_items() {
    let item = |text: &str| json!({"type": "listItem", "content": [{"type": "paragraph", "content": [{"type": "text", "text": text}]}]});
    assert_eq!(
        content("- a\n* b\n1. c\n2. d"),
        json!([
            {"type": "bulletList", "content": [item("a"), item("b")]},
            {"type": "orderedList", "content": [item("c"), item("d")]}
        ])
    );
}

#[test]
fn a_checklist_becomes_a_task_list_with_states_and_local_ids() {
    assert_eq!(
        content("- [ ] escrever testes\n- [x] revisar"),
        json!([{
            "type": "taskList",
            "attrs": {"localId": "list-3"},
            "content": [
                {"type": "taskItem", "attrs": {"localId": "task-1", "state": "TODO"}, "content": [{"type": "text", "text": "escrever testes"}]},
                {"type": "taskItem", "attrs": {"localId": "task-2", "state": "DONE"}, "content": [{"type": "text", "text": "revisar"}]}
            ]
        }])
    );
}

#[test]
fn a_fenced_block_keeps_its_language_and_lines_verbatim() {
    assert_eq!(
        content("```rust\nfn main() {\n    **not bold**\n}\n```\nafter"),
        json!([
            {"type": "codeBlock", "attrs": {"language": "rust"}, "content": [{"type": "text", "text": "fn main() {\n    **not bold**\n}"}]},
            {"type": "paragraph", "content": [{"type": "text", "text": "after"}]}
        ])
    );
}

#[test]
fn inline_code_links_bold_and_italic_become_marks() {
    assert_eq!(
        inline("run `cargo test` per [docs](https://x.dev/a_b) **now** *ok* snake_case [x] 2 * 3"),
        vec![
            json!({"type": "text", "text": "run "}),
            json!({"type": "text", "text": "cargo test", "marks": [{"type": "code"}]}),
            json!({"type": "text", "text": " per "}),
            json!({"type": "text", "text": "docs", "marks": [{"type": "link", "attrs": {"href": "https://x.dev/a_b"}}]}),
            json!({"type": "text", "text": " "}),
            json!({"type": "text", "text": "now", "marks": [{"type": "strong"}]}),
            json!({"type": "text", "text": " "}),
            json!({"type": "text", "text": "ok", "marks": [{"type": "em"}]}),
            json!({"type": "text", "text": " snake_case [x] 2 * 3"}),
        ]
    );
}

#[test]
fn a_bold_link_carries_both_marks() {
    assert_eq!(
        inline("**[PR](https://g/1)**"),
        vec![
            json!({"type": "text", "text": "PR", "marks": [{"type": "link", "attrs": {"href": "https://g/1"}}, {"type": "strong"}]})
        ]
    );
}

#[test]
fn rules_and_quotes_are_blocks() {
    assert_eq!(
        content("> citado\n---"),
        json!([
            {"type": "blockquote", "content": [{"type": "paragraph", "content": [{"type": "text", "text": "citado"}]}]},
            {"type": "rule"}
        ])
    );
}

#[test]
fn markdown_survives_a_round_trip_through_adf() {
    let markdown = "# Título\n\nTexto com `code`, **negrito** e [link](https://a.b).\n\n- um\n- dois\n\n1. primeiro\n2. segundo\n\n\
                    - [ ] pendente\n- [x] feito\n\n```sh\ncargo test\n```\n\n> nota\n\n---";
    assert_eq!(adf_to_markdown(&markdown_to_adf(markdown)), markdown);
}

#[test]
fn mentions_and_unknown_nodes_read_as_text() {
    let document = json!({"type": "doc", "version": 1, "content": [
        {"type": "paragraph", "content": [{"type": "mention", "attrs": {"id": "1", "text": "@Ana"}}, {"type": "text", "text": " veja"}]},
        {"type": "panel", "attrs": {"panelType": "info"}, "content": [{"type": "paragraph", "content": [{"type": "text", "text": "dentro"}]}]}
    ]});
    assert_eq!(adf_to_markdown(&document), "@Ana veja\n\ndentro");
    assert_eq!(adf_to_markdown(&Value::Null), "");
}
