use serde_json::json;

use super::*;

fn qwen() -> Format {
    Format {
        when: "<function=".into(),
        think: Some(Markers { open: "<think>".into(), close: "</think>".into() }),
        call: Some(CallMarkers { open: "<tool_call>".into(), close: "</tool_call>".into(), format: CallFormat::Xml }),
    }
}

fn tools() -> Value {
    json!([{ "type": "function", "function": { "name": "read", "parameters": { "properties": {
        "path": { "type": "string" }, "limit": { "type": "integer" }, "flags": { "type": "array" }
    } } } }])
}

fn split(pieces: &[&str], thinking: bool) -> Vec<Piece> {
    let mut splitter = Splitter::new(qwen(), tools(), thinking, Vec::new());
    let mut out = Vec::new();
    for piece in pieces {
        splitter.push(piece, &mut out);
    }
    splitter.finish(&mut out);
    merged(out)
}

fn merged(pieces: Vec<Piece>) -> Vec<Piece> {
    let mut joined: Vec<Piece> = Vec::new();
    for piece in pieces {
        match (joined.last_mut(), piece) {
            (Some(Piece::Content(text)), Piece::Content(more))
            | (Some(Piece::Reasoning(text)), Piece::Reasoning(more)) => {
                text.push_str(&more);
            }
            (_, piece) => joined.push(piece),
        }
    }
    joined
}

#[test]
fn a_prompt_that_opened_thinking_starts_the_answer_as_reasoning() {
    let out = split(&["let me ", "see</th", "ink>\n\nDone."], true);
    assert_eq!(out, [Piece::Reasoning("let me see".into()), Piece::Content("Done.".into())]);
}

#[test]
fn an_xml_call_reads_strings_raw_and_other_types_as_json() {
    let call = "<tool_call>\n<function=read>\n<parameter=path>\nsrc/a b.rs\n</parameter>\n<parameter=limit>\n20\n</parameter>\n<parameter=flags>\n[\"x\"]\n</parameter>\n</function>\n</tool_call>";
    let out = split(&["Reading.\n\n", &call[..9], &call[9..40], &call[40..]], false);
    assert_eq!(out[0], Piece::Content("Reading.\n\n".into()));
    let Piece::Call { name, arguments } = &out[1] else { panic!("no call in {out:?}") };
    assert_eq!(name, "read");
    let arguments: Value = serde_json::from_str(arguments).unwrap_or_default();
    assert_eq!(arguments, json!({ "path": "src/a b.rs", "limit": 20, "flags": ["x"] }));
}

#[test]
fn a_json_call_keeps_its_arguments() {
    let piece = parsed("{\"name\": \"read\", \"arguments\": {\"path\": \"a\"}}", CallFormat::Json, &Value::Null);
    assert_eq!(piece, Some(Piece::Call { name: "read".into(), arguments: "{\"path\":\"a\"}".into() }));
}

#[test]
fn a_stop_string_ends_the_answer_where_it_starts() {
    let mut splitter = Splitter::new(Format::default(), Value::Null, false, vec!["END".into()]);
    let mut out = Vec::new();
    splitter.push("one two E", &mut out);
    splitter.push("ND three", &mut out);
    splitter.finish(&mut out);
    assert!(splitter.stopped());
    assert_eq!(merged(out), [Piece::Content("one two ".into())]);
}

#[test]
fn the_first_format_whose_mark_is_in_the_template_is_chosen() {
    let json = Format { when: "<tool_call>".into(), ..Format::default() };
    let chosen = Format::chosen(&[qwen(), json.clone()], "... <tool_call> {\"name\": ...");
    assert_eq!(chosen, json);
    assert_eq!(Format::chosen(&[qwen()], "plain"), Format::default());
}
