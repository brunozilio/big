use std::path::Path;

use super::*;

const ONE: &str = "1111111111111111111111111111111111111111111111111111111111111111";
const TWO: &str = "2222222222222222222222222222222222222222222222222222222222222222";

fn line(json: serde_json::Value) -> String {
    format!("{json}\n")
}

fn prompted(at_ms: u64, text: &str) -> String {
    line(serde_json::json!({"kind": "prompted", "at_ms": at_ms, "prompt": text, "seq": at_ms}))
}

fn said(role: &str, text: &str) -> String {
    line(serde_json::json!({"kind": "message", "role": role, "blocks": [{"Text": text}], "seq": 1}))
}

fn opened(cwd: &str) -> String {
    line(serde_json::json!({"type": "trace", "kind": "opened", "at_ms": 1, "run": {
        "model": "m", "provider": "p", "permission": "x", "effort": "low", "cwd": cwd
    }, "seq": 2}))
}

fn write(dir: &Path, name: &str, text: &str) {
    std::fs::write(dir.join(name), text).expect("write");
}

fn shelf_of(dir: &Path) -> Shelf {
    Shelf::new(vec![Kept { name: "current".into(), dir: dir.to_path_buf() }])
}

fn everything(text: &str) -> Query {
    Query { text: text.into(), limit: 20, ..Query::default() }
}

#[test]
fn a_message_the_user_sent_is_found_with_its_session_time_and_folder() {
    let dir = tempfile::tempdir().expect("dir");
    let log = [
        prompted(5, "Vamos criar uma gateway de segurança em Go"),
        opened("/work/gateway"),
        said("User", "Vamos criar uma gateway de segurança em Go"),
        said("Assistant", "Começo pelo proxy reverso."),
    ]
    .concat();
    write(dir.path(), &format!("{ONE}.jsonl"), &log);
    write(dir.path(), &format!("{TWO}.jsonl"), &said("User", "outra coisa"));
    let hits = shelf_of(dir.path()).search(&everything("SEGURANCA gateway"));
    assert_eq!(hits.len(), 1, "{hits:?}");
    let hit = &hits[0];
    assert_eq!(hit.session.session, ONE);
    assert_eq!(hit.session.cwd.as_deref(), Some("/work/gateway"));
    assert_eq!((hit.role, hit.at_ms, hit.turn), ("user", Some(5), Some(0)), "the prompt and its turn are one hit");
    assert_eq!(hit.excerpt, "Vamos criar uma gateway de segurança em Go");
}

#[test]
fn the_catalog_names_a_session_and_the_folder_filter_keeps_only_its_sessions() {
    let dir = tempfile::tempdir().expect("dir");
    write(dir.path(), &format!("{ONE}.jsonl"), &[opened("/work/api"), said("User", "deploy plan")].concat());
    write(dir.path(), &format!("{TWO}.jsonl"), &[opened("/work/web"), said("User", "deploy plan")].concat());
    let shelf = shelf_of(dir.path()).named(ONE.into(), Some("Deploy".into()), "/work/api".into());
    let query = Query { cwd: Some("/work/api".into()), ..everything("deploy") };
    let hits = shelf.search(&query);
    assert_eq!(hits.iter().map(|hit| hit.session.session.as_str()).collect::<Vec<_>>(), vec![ONE]);
    assert_eq!(hits[0].session.title.as_deref(), Some("Deploy"));
    assert_eq!(shelf.sessions(&Query { limit: 10, ..Query::default() }).len(), 2);
}

#[test]
fn sessions_before_the_given_time_are_left_out() {
    let dir = tempfile::tempdir().expect("dir");
    write(dir.path(), &format!("{ONE}.jsonl"), &said("User", "x"));
    let later = Query { since_ms: Some(u64::MAX), limit: 10, ..Query::default() };
    assert!(shelf_of(dir.path()).sessions(&later).is_empty());
}

#[test]
fn side_files_and_other_names_are_not_sessions() {
    let dir = tempfile::tempdir().expect("dir");
    write(dir.path(), &format!("checkpoints-{ONE}.json"), "[]");
    write(dir.path(), &format!("{ONE}.jsonl.corrupt"), &said("User", "lost"));
    write(dir.path(), "notes.jsonl", &said("User", "lost"));
    assert!(shelf_of(dir.path()).sessions(&everything("")).is_empty());
}

#[test]
fn a_trace_kept_apart_and_an_old_whole_save_are_read_too() {
    let dir = tempfile::tempdir().expect("dir");
    write(dir.path(), &format!("{ONE}.trace.jsonl"), &[prompted(9, "where is the vault"), opened("/old")].concat());
    let saved = serde_json::json!([{"role": "User", "blocks": [{"Text": "the vault question"}]}]);
    write(dir.path(), &format!("{TWO}.json"), &saved.to_string());
    let hits = shelf_of(dir.path()).search(&everything("vault"));
    assert_eq!(hits.len(), 2, "{hits:?}");
    let read = shelf_of(dir.path()).read(&ONE[..8], 0, 10).expect("read");
    assert_eq!(read.turns.len(), 1, "a session with only prompts reads as its prompts");
    assert_eq!(read.turns[0].text, "where is the vault");
}

#[test]
fn reading_a_session_pages_its_turns() {
    let dir = tempfile::tempdir().expect("dir");
    let log: String =
        (0..5).map(|n| said(if n % 2 == 0 { "User" } else { "Assistant" }, &format!("turn {n}"))).collect();
    write(dir.path(), &format!("{ONE}.jsonl"), &log);
    let read = shelf_of(dir.path()).read(ONE, 2, 2).expect("read");
    assert_eq!(read.total, 5);
    assert_eq!(
        read.turns.iter().map(|turn| (turn.turn, turn.role)).collect::<Vec<_>>(),
        [(2, "user"), (3, "assistant")]
    );
    assert!(shelf_of(dir.path()).read("1111", 0, 1).is_err(), "too short an id");
    assert!(shelf_of(dir.path()).read(TWO, 0, 1).is_err(), "no such session");
}

#[test]
fn an_excerpt_shows_the_words_around_the_first_match() {
    let long = format!("{} needle {}", "a ".repeat(200), "b ".repeat(200));
    let words = vec![folded("needle")];
    let shown = excerpt(&long, &words).expect("found");
    assert!(shown.starts_with('…') && shown.ends_with('…') && shown.contains("needle"), "{shown}");
    assert_eq!(excerpt("nothing here", &words), None);
}
