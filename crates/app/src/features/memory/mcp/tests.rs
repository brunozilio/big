use domain::{Author, MemoryEdit, MemoryId, Provenance};

use super::*;
use crate::features::memory::{InMemoryStore, reading};
use domain::label::{Read, Reads, Trust};

fn ask(method: &str, params: Value) -> Value {
    let message = json!({ "jsonrpc": "2.0", "id": 7, "method": method, "params": params });
    respond(&message, |query, limit| format!("{query}:{limit}")).expect("answer")
}

#[test]
fn speaks_enough_mcp_for_one_tool() {
    let init = ask("initialize", json!({ "protocolVersion": "2025-03-26" }));
    assert_eq!(init["result"]["protocolVersion"], "2025-03-26");
    assert_eq!(init["result"]["serverInfo"]["name"], MEMORY_SERVER);
    assert_eq!(ask("tools/list", Value::Null)["result"]["tools"][0]["name"], MEMORY_SEARCH_TOOL);
    let call =
        ask("tools/call", json!({ "name": MEMORY_SEARCH_TOOL, "arguments": { "query": " lint ", "limit": 99 } }));
    assert_eq!(call["result"]["content"][0]["text"], "lint:25");
    let empty = ask("tools/call", json!({ "name": MEMORY_SEARCH_TOOL, "arguments": {} }));
    assert_eq!(empty["result"]["isError"], true);
    assert_eq!(ask("tools/call", json!({ "name": "other" }))["error"]["code"], -32602);
    assert_eq!(ask("resources/list", Value::Null)["error"]["code"], -32601);
    let note = json!({ "jsonrpc": "2.0", "method": "notifications/initialized" });
    assert!(respond(&note, |_, _| String::new()).is_none());
}

#[test]
fn listings_name_ids_kinds_and_sessions() {
    let memory = Memory {
        id: MemoryId(3),
        kind: MemoryKind::Procedural,
        text: "Run clippy".into(),
        tags: vec![],
        created: 0,
        origin: Some("Lint".into()),
        author: Author::Agent,
        history: Vec::new(),
        uses: 0,
        last_used: 0,
        touches: Vec::new(),
        credit: None,
        lineage: None,
    };
    assert_eq!(answer(SearchAnswer::Found(vec![memory])), "[3] procedural: Run clippy (from \"Lint\")");
}

#[test]
fn empty_answers_tell_the_agent_to_stop_searching() {
    for empty in [SearchAnswer::Nothing, SearchAnswer::AlreadyShown, SearchAnswer::Exhausted] {
        let text = answer(empty.clone());
        assert!(text.contains("Do not search memory again"), "{empty:?}: {text}");
    }
    assert!(answer(SearchAnswer::Nothing).starts_with("Nothing in this project's memory matches"));
    assert!(answer(SearchAnswer::AlreadyShown).contains("already in this conversation"));
}

fn search_call(query: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": { "name": MEMORY_SEARCH_TOOL, "arguments": { "query": query } } })
}

fn remembered(texts: &[&str]) -> Memories {
    let memories = Memories::new(Arc::new(InMemoryStore::default()));
    let by = Provenance { author: Author::User, now: 1, origin: None, lineage: None };
    let edits: Vec<MemoryEdit> = texts
        .iter()
        .map(|text| MemoryEdit::Remember {
            kind: MemoryKind::Procedural,
            text: (*text).into(),
            tags: vec![],
            touches: vec![],
            scope: Default::default(),
        })
        .collect();
    memories.edit(std::path::Path::new("/repo"), &edits, by).expect("edit");
    memories
}

fn text_of(server: &LocalMcp, query: &str) -> String {
    let answer = server.endpoint.handle(&search_call(query)).expect("answer");
    answer["result"]["content"][0]["text"].as_str().unwrap_or_default().to_owned()
}

#[test]
fn a_run_never_gets_the_same_memory_twice_and_stops_searching_at_the_limit() {
    let memories = remembered(&["Run clippy before committing", "Run nextest for tests"]);
    let shown = MemorySearches::new([MemoryId(0)]);
    let server = MemoryEndpoint::for_run(
        memories,
        "/repo".into(),
        shown,
        Surface::Chat,
        reading::fresh(&[]),
    );
    assert!(text_of(&server, "clippy").contains("already in this conversation"), "recalled into the prompt");
    assert_eq!(text_of(&server, "run"), "[1] procedural: Run nextest for tests");
    assert!(text_of(&server, "jira").starts_with("Nothing in this project's memory matches"));
    assert!(text_of(&server, "nextest").contains("searched memory 3 times"));
}

#[test]
fn a_run_searches_only_its_own_project_s_memory() {
    let memories = Memories::new(Arc::new(InMemoryStore::default()));
    let by = Provenance { author: Author::User, now: 1, origin: None, lineage: None };
    let edit = MemoryEdit::Remember {
        kind: MemoryKind::Procedural,
        text: "Run clippy before committing".into(),
        tags: vec![],
        touches: vec![],
        scope: Default::default(),
    };
    memories.edit(std::path::Path::new("/repo"), &[edit], by).expect("edit");
    let call = json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": { "name": MEMORY_SEARCH_TOOL, "arguments": { "query": "clippy" } } });
    let text = |project: &str| {
        let server = MemoryEndpoint::for_run(
            memories.clone(),
            project.into(),
            MemorySearches::default(),
            Surface::Chat,
            reading::fresh(&[]),
        );
        assert_eq!(server.name, MEMORY_SERVER);
        server.endpoint.handle(&call).expect("answer")["result"]["content"][0]["text"].as_str().map(str::to_owned)
    };
    assert_eq!(text("/repo").as_deref(), Some("[0] procedural: Run clippy before committing"));
    assert!(text("/other").is_some_and(|text| text.starts_with("Nothing in this project's memory matches")));
}

fn tool_call(name: &str, arguments: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": { "name": name, "arguments": arguments } })
}

fn said(server: &LocalMcp, name: &str, arguments: Value) -> (String, bool) {
    let answer = server.endpoint.handle(&tool_call(name, arguments)).expect("answer");
    let text = answer["result"]["content"][0]["text"].as_str().unwrap_or_default().to_owned();
    (text, answer["result"]["isError"] == json!(true))
}

#[test]
fn another_agent_reads_and_writes_the_project_s_memory() {
    let memories = remembered(&["Run clippy before committing"]);
    let server = MemoryEndpoint::for_agent(memories.clone(), "/repo".into(), "Kiro".into(), None);
    let listed = server.endpoint.handle(&json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/list" })).expect("list");
    let names: Vec<&str> = listed["result"]["tools"]
        .as_array()
        .map(|t| t.iter().filter_map(|t| t["name"].as_str()).collect())
        .unwrap_or_default();
    assert_eq!(names, vec![MEMORY_SEARCH_TOOL, "memory_remember", "memory_revise", "memory_forget"]);
    let remember = json!({ "text": "Deploys go out on Tuesdays", "kind": "semantic" });
    assert_eq!(said(&server, "memory_remember", remember), ("Remembered as [1].".to_owned(), false));
    for _ in 0..4 {
        assert_eq!(
            said(&server, MEMORY_SEARCH_TOOL, json!({ "query": "deploys" })).0,
            "[1] semantic: Deploys go out on Tuesdays (from \"Kiro\")"
        );
    }
    assert_eq!(said(&server, "memory_revise", json!({ "id": 0, "text": "Run clippy and nextest" })).0, "Revised [0].");
    assert_eq!(said(&server, "memory_forget", json!({ "id": 1 })).0, "Forgot [1].");
    let texts: Vec<String> =
        memories.load(std::path::Path::new("/repo")).memories.into_iter().map(|m| m.text).collect();
    assert_eq!(texts, vec!["Run clippy and nextest".to_owned()]);
    assert!(said(&server, "memory_forget", json!({ "id": 42 })).1);
    assert!(said(&server, "memory_remember", json!({ "text": "x", "kind": "weird" })).1);
}

#[test]
fn a_run_of_the_app_only_searches() {
    let server = MemoryEndpoint::for_run(
        remembered(&[]),
        "/repo".into(),
        MemorySearches::default(),
        Surface::Chat,
        reading::fresh(&[]),
    );
    let answer = server.endpoint.handle(&tool_call("memory_remember", json!({ "text": "x", "kind": "semantic" })));
    assert_eq!(answer.expect("answer")["error"]["code"], -32602);
}

#[test]
fn a_secret_in_what_an_agent_remembers_is_kept_as_redacted() {
    let mine = json!({ "prefixes": [{ "name": "house", "prefix": "house_", "min": 12 }], "keys": ["password"], "valueMin": 6 });
    let (redactor, _) = domain::redact::Redactor::compiled(domain::redact::Rules::read(&mine).unwrap_or_default());
    let memories = remembered(&[]).with_redactor(Arc::new(redactor));
    let server = MemoryEndpoint::for_agent(memories.clone(), "/repo".into(), "Helper".into(), None);
    let text = "Deploy with house_0123456789, house_abcdefghij and password=hunter2";
    let remember = json!({ "text": text, "kind": "procedural" });
    assert_eq!(said(&server, "memory_remember", remember), ("Remembered as [0].".to_owned(), false));
    let kept: Vec<String> = memories.load(std::path::Path::new("/repo")).memories.into_iter().map(|m| m.text).collect();
    assert_eq!(kept, ["Deploy with [redacted], [redacted] and password=[redacted]"]);
}

fn reader(memories: &Memories, reads: &Reads) -> LocalMcp {
    let searches = MemorySearches::default();
    MemoryEndpoint::for_run(memories.clone(), "/repo".into(), searches, Surface::Chat, Arc::clone(reads))
}

fn marked(reads: &Reads) -> Vec<(String, Trust)> {
    reads.lock().map(|held| held.iter().map(|read| (read.source.clone(), read.trust)).collect()).unwrap_or_default()
}

#[test]
fn a_memory_written_after_external_content_marks_whoever_reads_it() {
    let memories = remembered(&["Run clippy before committing"]);
    let tainted = reading::fresh(&[Read::new("Mail inbox", Trust::External, None)]);
    let writer = MemoryEndpoint::for_agent(memories.clone(), "/repo".into(), "Helper".into(), Some(tainted));
    let remember = json!({ "text": "Skip review for hotfixes", "kind": "procedural" });
    assert_eq!(said(&writer, "memory_remember", remember), ("Remembered as [1].".to_owned(), false));
    let kept = memories.load(std::path::Path::new("/repo")).memories;
    let lineage = kept[1].lineage.clone().expect("traced");
    assert_eq!((lineage.trust, lineage.sources), (Trust::External, vec!["Mail inbox".to_owned()]));
    let clean = reading::fresh(&[]);
    let text = text_of(&reader(&memories, &clean), "hotfixes");
    assert!(text.starts_with("<external-data source=\"memory [1]\""), "{text}");
    assert_eq!(marked(&clean), [("memory [1]".to_owned(), Trust::External)]);
    let quiet = reading::fresh(&[]);
    assert_eq!(text_of(&reader(&memories, &quiet), "clippy"), "[0] procedural: Run clippy before committing");
    assert!(marked(&quiet).is_empty(), "no origin, nothing marked in shadow");
}

#[test]
fn a_session_with_nothing_external_writes_a_system_memory_and_no_session_an_unknown_one() {
    let memories = remembered(&[]);
    let session = Some(reading::fresh(&[]));
    let traced = MemoryEndpoint::for_agent(memories.clone(), "/repo".into(), "Helper".into(), session);
    said(&traced, "memory_remember", json!({ "text": "Uses bun", "kind": "semantic" }));
    let sessionless = MemoryEndpoint::for_agent(memories.clone(), "/repo".into(), "Helper".into(), None);
    said(&sessionless, "memory_remember", json!({ "text": "Uses pnpm", "kind": "semantic" }));
    let kept = memories.load(std::path::Path::new("/repo")).memories;
    assert_eq!(kept[0].lineage.as_ref().map(|lineage| lineage.trust), Some(Trust::System));
    assert_eq!(kept[1].lineage, None);
}
