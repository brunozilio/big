use serde_json::json;

use super::shipped;
use crate::tools::read_files::Freshness;
use crate::tools::{Effect, Outcome, Tool, Workspace};

#[test]
fn glob_matches_paths_skips_ignored_files_and_lists_the_newest_first() {
    let dir = tempfile::tempdir().expect("dir");
    let root = dir.path();
    std::fs::create_dir_all(root.join("src/deep")).expect("dirs");
    std::fs::create_dir_all(root.join("target")).expect("dirs");
    std::fs::create_dir(root.join(".git")).expect("git dir");
    for file in ["src/a.rs", "src/deep/b.rs", "src/c.md", "target/d.rs"] {
        std::fs::write(root.join(file), "").expect("write");
    }
    std::fs::write(root.join(".gitignore"), "target/\n").expect("write");
    let workspace = Workspace::new(root);
    let glob = shipped("Glob");
    assert!(glob.spec.description.contains("up to 100 paths"), "{}", glob.spec.description);
    assert_eq!(glob.summary(&json!({ "pattern": "**/*.rs" })), "**/*.rs");

    let outcome = glob.run(&json!({ "pattern": "**/*.rs" }), &workspace);
    let normalized = outcome.content.replace('\\', "/");
    let mut paths: Vec<&str> = normalized.lines().collect();
    paths.sort_unstable();
    assert_eq!(paths, vec!["src/a.rs", "src/deep/b.rs"]);

    let shallow = glob.run(&json!({ "pattern": "*.rs", "path": "src" }), &workspace);
    assert_eq!(shallow.content.replace('\\', "/"), "src/a.rs");
    assert_eq!(glob.run(&json!({ "pattern": "*.py" }), &workspace).content, "No files found.");
    let bad = glob.run(&json!({ "pattern": "[" }), &workspace);
    assert!(bad.is_error && bad.content.contains("Invalid pattern"), "{}", bad.content);
}

#[test]
fn a_built_in_script_cannot_reach_past_what_it_declared() {
    let dir = tempfile::tempdir().expect("dir");
    std::fs::create_dir_all(dir.path().join("src")).expect("src");
    std::fs::write(dir.path().join("secret.txt"), "s").expect("secret");
    let workspace = Workspace::new(dir.path());
    let glob = shipped("Glob");
    let effects = glob.effects(&json!({ "pattern": "*", "path": "src" }), &workspace);
    assert_eq!(effects, vec![Effect::ReadTree(workspace.place("src"))]);
    let outcome = glob.run(&json!({ "pattern": "*", "path": "src" }), &workspace);
    assert_eq!(outcome.content, "No files found.");
}

fn read_text(workspace: &Workspace, input: serde_json::Value) -> String {
    shipped("Read").run(&input, workspace).content
}

fn project() -> (tempfile::TempDir, Workspace) {
    let dir = tempfile::tempdir().expect("dir");
    std::fs::write(dir.path().join("a.rs"), "one\ntwo\nthree\n").expect("write");
    let workspace = Workspace::new(dir.path());
    (dir, workspace)
}

#[test]
fn read_numbers_lines_reads_parts_and_marks_the_file_read() {
    let dir = tempfile::tempdir().expect("dir");
    std::fs::write(dir.path().join("a.txt"), "one\ntwo\nthree\n").expect("write");
    let workspace = Workspace::new(dir.path());
    let read = shipped("Read");

    let part = read.run(&json!({"file_path": "a.txt", "offset": 2, "limit": 1}), &workspace);
    assert_eq!(part.content, "     2\ttwo\n… [1 more lines; read on from offset 3]\n");

    let whole = read.run(&json!({"file_path": "a.txt"}), &workspace);
    assert_eq!(whole, Outcome::ok("     1\tone\n     2\ttwo\n     3\tthree\n"));
    assert!(workspace.has_read(&dir.path().join("a.txt")));
}

#[test]
fn read_counts_a_slice_as_a_partial_read() {
    let dir = tempfile::tempdir().expect("dir");
    std::fs::write(dir.path().join("a.txt"), "one\ntwo\nthree\n").expect("write");
    let workspace = Workspace::new(dir.path());
    shipped("Read").run(&json!({"file_path": "a.txt", "limit": 2}), &workspace);
    assert_eq!(workspace.freshness(&dir.path().join("a.txt")), Freshness::Partial);
    shipped("Read").run(&json!({"file_path": "a.txt"}), &workspace);
    assert_eq!(workspace.freshness(&dir.path().join("a.txt")), Freshness::Current);
}

#[test]
fn read_explains_what_it_cannot_read() {
    let dir = tempfile::tempdir().expect("dir");
    let workspace = Workspace::new(dir.path());
    let read = shipped("Read");
    assert!(read.run(&json!({"file_path": "missing"}), &workspace).is_error);
    assert!(read.run(&json!({"file_path": "."}), &workspace).content.contains("directory"));
    assert!(read.run(&json!({}), &workspace).content.contains("file_path"));
    std::fs::write(dir.path().join("b"), [0xff, 0xfe]).expect("write");
    assert!(read.run(&json!({"file_path": "b"}), &workspace).content.contains("not a text file"));
}

#[test]
fn reading_an_unchanged_file_again_sends_a_note_not_its_text() {
    let (_dir, workspace) = project();
    assert!(read_text(&workspace, json!({"file_path": "a.rs"})).contains("two"));
    let again = read_text(&workspace, json!({"file_path": "a.rs"}));
    assert!(!again.contains("two") && again.contains("unchanged"), "{again}");
}

#[test]
fn lines_inside_what_was_already_read_send_a_note_not_their_text() {
    let (dir, workspace) = project();
    read_text(&workspace, json!({"file_path": "a.rs"}));
    let inside = read_text(&workspace, json!({"file_path": "a.rs", "offset": 2, "limit": 1}));
    assert!(!inside.contains("two") && inside.contains("unchanged"), "{inside}");

    let line = "x".repeat(1_000);
    std::fs::write(dir.path().join("big.txt"), format!("{line}\n").repeat(500)).expect("write");
    read_text(&workspace, json!({"file_path": "big.txt"}));
    let past_the_cut = read_text(&workspace, json!({"file_path": "big.txt", "offset": 400, "limit": 2}));
    assert!(past_the_cut.contains("   400\tx"), "only the lines shown count as read");
}

#[test]
fn a_changed_file_or_lines_not_read_yet_are_read_in_full() {
    let (dir, workspace) = project();
    read_text(&workspace, json!({"file_path": "a.rs", "limit": 1}));
    assert!(read_text(&workspace, json!({"file_path": "a.rs", "offset": 2})).contains("two"));
    std::fs::write(dir.path().join("a.rs"), "one\ntwo\nthree\nfour\n").expect("write");
    assert!(read_text(&workspace, json!({"file_path": "a.rs", "offset": 2})).contains("four"));
}

#[test]
fn a_huge_read_stops_at_a_whole_line_and_says_where_to_go_on() {
    let (dir, workspace) = project();
    let line = "x".repeat(1_000);
    std::fs::write(dir.path().join("big.txt"), format!("{line}\n").repeat(500)).expect("write");
    let text = read_text(&workspace, json!({"file_path": "big.txt"}));
    assert!(text.len() < 100_000 + 500, "{}", text.len());
    let next = text.lines().filter(|l| l.contains('\t')).count() + 1;
    assert!(text.ends_with(&format!("read on from offset {next}]\n")), "{}", &text[text.len() - 200..]);
}

#[test]
fn a_sub_agent_or_a_compacted_conversation_gets_the_text_again() {
    let (_dir, workspace) = project();
    read_text(&workspace, json!({"file_path": "a.rs"}));
    let child = workspace.fork(std::sync::Arc::default());
    assert!(read_text(&child, json!({"file_path": "a.rs"})).contains("two"), "the child never saw it");
    workspace.forget_seen_files();
    assert!(read_text(&workspace, json!({"file_path": "a.rs"})).contains("two"), "the summary dropped it");
}

#[test]
fn read_sends_an_image_as_an_image_and_long_lines_are_cut() {
    let (dir, workspace) = project();
    std::fs::write(dir.path().join("a.png"), b"\x89PNG\r\n\x1a\n rest").expect("png");
    let image = shipped("Read").run(&json!({"file_path": "a.png"}), &workspace);
    assert_eq!(image.content, "Image (image/png)");
    assert_eq!(image.images.len(), 1);
    assert!(workspace.has_read(&dir.path().join("a.png")));
    std::fs::write(dir.path().join("wide.txt"), format!("{}\n", "é".repeat(2_005))).expect("wide");
    let wide = read_text(&workspace, json!({"file_path": "wide.txt"}));
    assert_eq!(wide, format!("     1\t{}… [line cut]\n", "é".repeat(2_000)));
    std::fs::write(dir.path().join("empty.txt"), "").expect("empty");
    assert_eq!(read_text(&workspace, json!({"file_path": "empty.txt"})), "(the file is empty)");
    let past = shipped("Read").run(&json!({"file_path": "a.rs", "offset": 9}), &workspace);
    assert_eq!(past, Outcome::error("The file has 3 lines; offset 9 is past its end."));
}

#[test]
fn pdf_pages_are_one_or_a_short_range() {
    let (dir, workspace) = project();
    std::fs::write(dir.path().join("doc.pdf"), b"%PDF-1.4 not really").expect("pdf");
    for pages in ["0", "5-1", "1-90", "first"] {
        let refused = shipped("Read").run(&json!({"file_path": "doc.pdf", "pages": pages}), &workspace);
        assert!(
            refused.is_error && refused.content.starts_with("pages must be a page or a range"),
            "{pages}: {refused:?}"
        );
    }
    for pages in ["3", "1-5"] {
        let asked = shipped("Read").run(&json!({"file_path": "doc.pdf", "pages": pages}), &workspace);
        assert!(!asked.content.contains("pages must be"), "{pages}: {asked:?}");
    }
    assert!(workspace.has_read(&dir.path().join("doc.pdf")));
}

#[test]
fn read_in_raw_mode_gives_the_text_as_it_is() {
    let (dir, workspace) = project();
    let raw = crate::tools::raw_text::reading(|| shipped("Read").run(&json!({"file_path": "a.rs"}), &workspace));
    assert_eq!(raw, Outcome::ok("one\ntwo\nthree\n"));
    assert!(workspace.has_read(&dir.path().join("a.rs")));
    let folder = crate::tools::raw_text::reading(|| shipped("Read").run(&json!({"file_path": "."}), &workspace));
    assert!(folder.is_error && folder.content.ends_with("is a directory, not a file."), "{folder:?}");
}

fn write(workspace: &Workspace, input: serde_json::Value) -> Outcome {
    shipped("Write").run(&input, workspace)
}

#[test]
fn write_creates_new_files_and_overwrites_only_what_was_read() {
    let dir = tempfile::tempdir().expect("dir");
    let workspace = Workspace::new(dir.path());
    let created = write(&workspace, json!({"file_path": "deep/new.txt", "content": "a\nb"}));
    assert_eq!(created.content, "Created deep/new.txt (2 lines).");
    let edited = created.edited.expect("edited");
    assert_eq!((edited.path, edited.line, edited.removed, edited.added), (dir.path().join("deep/new.txt"), 1, 0, 2));

    std::fs::write(dir.path().join("old.txt"), "keep").expect("write");
    let blind = write(&workspace, json!({"file_path": "old.txt", "content": "x"}));
    assert!(blind.is_error);
    assert_eq!(std::fs::read_to_string(dir.path().join("old.txt")).expect("read"), "keep");

    workspace.mark_read(&dir.path().join("old.txt"));
    let replaced = write(&workspace, json!({"file_path": "old.txt", "content": "x"}));
    assert_eq!(replaced.content, "Replaced old.txt (1 lines).");
    assert_eq!(replaced.edited.expect("edited").removed, 1);
}

#[test]
fn replacing_a_file_too_large_to_keep_says_that_rewind_cannot_put_it_back() {
    let dir = tempfile::tempdir().expect("dir");
    let mut workspace = Workspace::new(dir.path());
    workspace.services.files = Some(std::sync::Arc::default());
    let (big, small) = (dir.path().join("big.log"), dir.path().join("small.txt"));
    std::fs::write(&big, vec![b'x'; 10 * 1024 * 1024 + 1]).expect("big");
    std::fs::write(&small, "one").expect("small");
    workspace.mark_read(&big);
    workspace.mark_read(&small);

    let replaced = write(&workspace, json!({"file_path": "big.log", "content": "short"}));
    let kept = write(&workspace, json!({"file_path": "small.txt", "content": "two"}));

    assert!(!replaced.is_error, "{replaced:?}");
    assert!(replaced.content.contains("/rewind cannot put back the earlier version"), "{}", replaced.content);
    assert!(replaced.content.contains("10 MiB"), "{}", replaced.content);
    assert!(!kept.content.contains("/rewind"), "{}", kept.content);
}

#[test]
fn a_part_read_or_changed_file_is_not_overwritten() {
    let dir = tempfile::tempdir().expect("dir");
    let workspace = Workspace::new(dir.path());
    let path = dir.path().join("big.txt");
    std::fs::write(&path, "head\ntail\n").expect("write");
    workspace.mark_partly_read(&path);
    let partial = write(&workspace, json!({"file_path": "big.txt", "content": "x"}));
    assert!(partial.is_error && partial.content.contains("only part of big.txt"), "{partial:?}");
    workspace.mark_read(&path);
    std::fs::write(&path, "head\ntail\nmore\n").expect("the user edits");
    let changed = write(&workspace, json!({"file_path": "big.txt", "content": "x"}));
    assert!(changed.is_error && changed.content.contains("changed on disk"), "{changed:?}");
    assert_eq!(std::fs::read_to_string(&path).expect("read"), "head\ntail\nmore\n");
}

#[test]
fn a_written_file_s_problems_follow_and_other_files_are_left_alone() {
    let dir = tempfile::tempdir().expect("dir");
    let mut workspace = Workspace::new(dir.path());
    let server = crate::lsp::fake::ScriptedServer::answering([]);
    *server.diagnostics.lock().expect("diagnostics") = Some(vec![json!({"severity": 2, "message": "unused",
        "range": {"start": {"line": 1, "character": 0}, "end": {"line": 1, "character": 3}}})]);
    workspace.services.lsp = Some(server.lsp());
    let written = write(&workspace, json!({"file_path": "src/a.py", "content": "import os\nx = 1\n"}));
    assert_eq!(
        written.content,
        "Created src/a.py (2 lines).\n\n<diagnostics>\nsrc/a.py:2:1 warning: unused\n</diagnostics>"
    );
    let notes = write(&workspace, json!({"file_path": "notes.md", "content": "hi"}));
    assert_eq!(notes.content, "Created notes.md (1 lines).");
    assert_eq!(server.synced.lock().expect("synced").len(), 1);
}

#[test]
fn a_new_file_is_all_added_and_long_ones_are_cut() {
    let dir = tempfile::tempdir().expect("dir");
    let workspace = Workspace::new(dir.path());
    let created = write(&workspace, json!({"file_path": "a.txt", "content": "one\ntwo"}));
    assert_eq!(created.shown.as_deref(), Some("Created a.txt · Wrote 2 lines\n    1 + one\n    2 + two"));
    let long = (0..510).map(|n| n.to_string()).collect::<Vec<_>>().join("\n");
    let cut = write(&workspace, json!({"file_path": "b.txt", "content": long}));
    assert!(cut.shown.unwrap_or_default().ends_with("… +10 lines"));
}

fn setup(text: &str) -> (tempfile::TempDir, Workspace) {
    let dir = tempfile::tempdir().expect("dir");
    std::fs::write(dir.path().join("f.rs"), text).expect("write");
    let workspace = Workspace::new(dir.path());
    workspace.mark_read(&dir.path().join("f.rs"));
    (dir, workspace)
}

fn content(dir: &tempfile::TempDir) -> String {
    std::fs::read_to_string(dir.path().join("f.rs")).expect("read")
}

fn edit(workspace: &Workspace, input: serde_json::Value) -> Outcome {
    shipped("Edit").run(&input, workspace)
}

#[test]
fn edit_replaces_a_unique_string() {
    let (dir, workspace) = setup("let a = 1;\nlet b = 2;\n");
    let outcome = edit(&workspace, json!({"file_path": "f.rs", "old_string": "b = 2", "new_string": "b = 3"}));
    assert_eq!(outcome.content, "Edited f.rs.");
    assert_eq!(content(&dir), "let a = 1;\nlet b = 3;\n");
    let edited = outcome.edited.expect("edited");
    assert_eq!((edited.path, edited.line, edited.removed, edited.added), (dir.path().join("f.rs"), 2, 1, 1));
}

#[test]
fn an_edit_written_with_lf_lines_matches_a_crlf_file_and_keeps_it_crlf() {
    let (dir, workspace) = setup("fn a() {\r\n    1\r\n}\r\n");
    let change = json!({"file_path": "f.rs", "old_string": "{\n    1\n}", "new_string": "{\n    2\n    3\n}"});
    assert_eq!(edit(&workspace, change).content, "Edited f.rs.");
    assert_eq!(content(&dir), "fn a() {\r\n    2\r\n    3\r\n}\r\n");
    let crlf_only = edit(&workspace, json!({"file_path": "f.rs", "old_string": "3\n}", "new_string": "3\r\n}"}));
    assert!(crlf_only.is_error && crlf_only.content.contains("line endings"), "{crlf_only:?}");
    assert_eq!(content(&dir), "fn a() {\r\n    2\r\n    3\r\n}\r\n");
}

#[test]
fn an_lf_file_and_one_without_a_trailing_newline_stay_as_they_were() {
    let (dir, workspace) = setup("a\nb");
    let outcome = edit(&workspace, json!({"file_path": "f.rs", "old_string": "a\nb", "new_string": "c\nd"}));
    assert!(!outcome.is_error, "{outcome:?}");
    assert_eq!(content(&dir), "c\nd");
}

#[test]
fn edit_refuses_ambiguous_missing_and_unread() {
    let (dir, workspace) = setup("x x");
    let twice = edit(&workspace, json!({"file_path": "f.rs", "old_string": "x", "new_string": "y"}));
    assert!(twice.content.contains("2 times"));
    let missing = edit(&workspace, json!({"file_path": "f.rs", "old_string": "z", "new_string": "y"}));
    assert!(missing.content.contains("not found"));
    assert_eq!(content(&dir), "x x");

    let all = edit(&workspace, json!({"file_path": "f.rs", "old_string": "x", "new_string": "y", "replace_all": true}));
    assert_eq!(all.content, "Edited f.rs (2 replacements).");

    std::fs::write(dir.path().join("g.rs"), "x").expect("write");
    let unread = edit(&workspace, json!({"file_path": "g.rs", "old_string": "x", "new_string": "y"}));
    assert!(unread.content.starts_with("Read g.rs"));
}

#[test]
fn an_empty_old_string_is_refused_whether_or_not_every_match_is_replaced() {
    let (dir, workspace) = setup("ab");
    for all in [true, false] {
        let outcome =
            edit(&workspace, json!({"file_path": "f.rs", "old_string": "", "new_string": "-", "replace_all": all}));
        assert!(outcome.is_error && outcome.content.contains("`old_string` is empty"), "{all}: {outcome:?}");
        assert_eq!(content(&dir), "ab", "replace_all {all}");
    }
}

#[test]
fn a_file_changed_since_it_was_read_must_be_read_again_but_its_own_edits_do_not_count() {
    let (dir, workspace) = setup("a = 1\n");
    let first = edit(&workspace, json!({"file_path": "f.rs", "old_string": "1", "new_string": "2"}));
    let second = edit(&workspace, json!({"file_path": "f.rs", "old_string": "2", "new_string": "3"}));
    assert!(!first.is_error && !second.is_error, "{second:?}");
    std::fs::write(dir.path().join("f.rs"), "a = 3\nb = 4\n").expect("the user edits");
    let stale = edit(&workspace, json!({"file_path": "f.rs", "old_string": "3", "new_string": "5"}));
    assert!(stale.is_error && stale.content.contains("changed on disk since you last read it"), "{stale:?}");
    assert_eq!(content(&dir), "a = 3\nb = 4\n");
}

#[test]
fn the_language_server_s_errors_follow_the_edit_and_the_diff_stays_the_same() {
    let (dir, mut workspace) = setup("let a: u32 = 1;\n");
    let server = crate::lsp::fake::ScriptedServer::answering([]);
    *server.diagnostics.lock().expect("diagnostics") = Some(vec![json!({"severity": 1, "message": "mismatched types",
        "range": {"start": {"line": 0, "character": 13}, "end": {"line": 0, "character": 16}}})]);
    workspace.services.lsp = Some(server.lsp());
    let outcome = edit(&workspace, json!({"file_path": "f.rs", "old_string": "1", "new_string": "\"one\""}));
    assert_eq!(outcome.content, "Edited f.rs.\n\n<diagnostics>\nf.rs:1:14 error: mismatched types\n</diagnostics>");
    assert!(!outcome.shown.unwrap_or_default().contains("diagnostics"));
    let synced = server.synced.lock().expect("synced");
    assert_eq!(synced[0], (dir.path().join("f.rs"), "rust".to_owned(), "let a: u32 = \"one\";\n".to_owned()));
    drop(synced);
    *server.diagnostics.lock().expect("diagnostics") = Some(Vec::new());
    let clean = edit(&workspace, json!({"file_path": "f.rs", "old_string": "\"one\"", "new_string": "2"}));
    assert_eq!(clean.content, "Edited f.rs.");
}

#[test]
fn an_edit_shows_only_what_changed_with_its_line_numbers() {
    let (_dir, workspace) = setup("fn a() {}\nfn main() {\n    let x = 1;\n    run(x);\n}\n");
    let change = json!({"file_path": "f.rs", "old_string": "    let x = 1;\n    run(x);", "new_string": "    let x = 2;\n    run(x);"});
    assert_eq!(
        edit(&workspace, change).shown.as_deref(),
        Some(
            "Edited f.rs · Added 1 lines, removed 1\n    3 -     let x = 1;\n    3 +     let x = 2;\n    4       run(x);"
        )
    );
}

const NOTEBOOK: &str = r##"{"cells":[
    {"cell_type":"markdown","id":"a","metadata":{},"source":["# Title\n"]},
    {"cell_type":"code","id":"b","metadata":{},"execution_count":1,"source":["print(1)\n","x = 2"],
     "outputs":[{"output_type":"stream","name":"stdout","text":["1\n"]}]}
],"metadata":{},"nbformat":4,"nbformat_minor":5}"##;

fn notebook(text: &str) -> (tempfile::TempDir, Workspace) {
    let dir = tempfile::tempdir().expect("dir");
    std::fs::write(dir.path().join("n.ipynb"), text).expect("write");
    let workspace = Workspace::new(dir.path());
    (dir, workspace)
}

fn edited_notebook(text: &str, change: serde_json::Value) -> Result<String, String> {
    let (dir, workspace) = notebook(text);
    workspace.mark_read(&dir.path().join("n.ipynb"));
    let mut input = change;
    input["notebook_path"] = json!("n.ipynb");
    let outcome = shipped("NotebookEdit").run(&input, &workspace);
    if outcome.is_error {
        return Err(outcome.content);
    }
    assert!(outcome.content.starts_with("Updated ") && outcome.content.ends_with("n.ipynb."), "{}", outcome.content);
    Ok(std::fs::read_to_string(dir.path().join("n.ipynb")).expect("read"))
}

fn rendered(text: &str) -> String {
    let (_dir, workspace) = notebook(text);
    read_text(&workspace, json!({"file_path": "n.ipynb"}))
}

#[test]
fn cells_read_with_their_outputs() {
    assert_eq!(
        rendered(NOTEBOOK),
        "<cell 0 id=a markdown>\n# Title\n</cell>\n<cell 1 id=b code>\nprint(1)\nx = 2\n<output>\n1\n</output>\n</cell>\n"
    );
}

#[test]
fn a_cell_is_replaced_inserted_or_deleted() {
    let replaced = edited_notebook(NOTEBOOK, json!({"cell_number": 1, "new_source": "print(3)"})).expect("replace");
    assert!(rendered(&replaced).contains("<cell 1 id=b code>\nprint(3)\n</cell>"), "outputs cleared");
    let insert = json!({"edit_mode": "insert", "cell_number": 0, "new_source": "y = 1", "cell_type": "code"});
    let inserted = edited_notebook(NOTEBOOK, insert).expect("insert");
    assert!(rendered(&inserted).contains("<cell 1 code>\ny = 1"));
    let deleted = edited_notebook(NOTEBOOK, json!({"edit_mode": "delete", "cell_number": 0})).expect("delete");
    assert!(rendered(&deleted).starts_with("<cell 0 id=b code>"));
    assert!(edited_notebook(NOTEBOOK, json!({"edit_mode": "delete", "cell_number": 9})).is_err());
    let (_dir, workspace) = notebook(NOTEBOOK);
    let unread = shipped("NotebookEdit").run(&json!({"notebook_path": "n.ipynb", "cell_number": 0}), &workspace);
    assert_eq!(unread, Outcome::error("Read the notebook before editing it."));
}

#[test]
fn an_edited_notebook_keeps_its_line_endings_and_trailing_newline_state() {
    let crlf = NOTEBOOK.replace('\n', "\r\n") + "\r\n";
    let edited =
        edited_notebook(&crlf, json!({"cell_number": 1, "new_source": "print(3)\nprint(4)"})).expect("replace");
    assert!(edited.ends_with("}\r\n"), "{edited:?}");
    assert_eq!(edited.matches('\n').count(), edited.matches("\r\n").count(), "every line ends with CRLF");
    assert!(rendered(&edited).contains("print(3)\nprint(4)"), "the cell's own text is unchanged");
    let bare = edited_notebook(NOTEBOOK, json!({"cell_number": 1, "new_source": "print(3)"})).expect("replace");
    assert!(bare.ends_with('}') && !bare.contains('\r'), "{bare:?}");
}

#[test]
fn an_edited_notebook_is_pretty_printed_in_the_order_its_keys_came() {
    let edited = edited_notebook(NOTEBOOK, json!({"cell_number": 1, "new_source": "print(3)"})).expect("replace");
    let mut expected: serde_json::Value = serde_json::from_str(NOTEBOOK).expect("notebook");
    expected["cells"][1]["source"] = json!(["print(3)"]);
    expected["cells"][1]["outputs"] = json!([]);
    expected["cells"][1]["execution_count"] = serde_json::Value::Null;
    assert_eq!(serde_json::from_str::<serde_json::Value>(&edited).expect("json"), expected);
    assert!(
        edited.starts_with("{\n  \"cells\": [\n    {\n      \"cell_type\": \"markdown\",\n      \"id\": \"a\""),
        "{edited}"
    );
}

fn grep_workspace() -> (tempfile::TempDir, Workspace) {
    let dir = tempfile::tempdir().expect("dir");
    std::fs::create_dir(dir.path().join("src")).expect("dir");
    std::fs::write(dir.path().join("src/a.rs"), "fn main() {\n    run();\n}\n").expect("write");
    std::fs::write(dir.path().join("src/b.md"), "Run it\nrun again\n").expect("write");
    let workspace = Workspace::new(dir.path());
    (dir, workspace)
}

fn grep(input: &serde_json::Value, workspace: &Workspace) -> Outcome {
    shipped("Grep").run(input, workspace)
}

fn grep_in(dir: &tempfile::TempDir, input: &serde_json::Value) -> String {
    grep(input, &Workspace::new(dir.path())).content
}

#[test]
fn grep_lists_files_counts_and_lines() {
    let (_dir, workspace) = grep_workspace();
    let files: Vec<String> =
        grep(&json!({"pattern": "run"}), &workspace).content.lines().map(|line| line.replace('\\', "/")).collect();
    assert_eq!(files, vec!["src/a.rs", "src/b.md"]);
    let blank = grep(&json!({"pattern": "run", "glob": "", "path": ""}), &workspace);
    assert_eq!(blank.content.replace('\\', "/"), "src/a.rs\nsrc/b.md", "empty optional fields count as left out");

    let count = grep(&json!({"pattern": "run", "-i": true, "glob": "*.md", "output_mode": "count"}), &workspace);
    assert_eq!(count.content.replace('\\', "/"), "src/b.md:2");

    let content = grep(&json!({"pattern": "run\\(", "output_mode": "content", "-n": true, "-B": 1}), &workspace);
    assert_eq!(content.content.replace('\\', "/"), "src/a.rs\n1-fn main() {\n2:    run();");
}

#[test]
fn matching_lines_are_grouped_under_their_file_in_path_order() {
    let dir = tempfile::tempdir().expect("dir");
    std::fs::write(dir.path().join("b.txt"), "hit three\n").expect("write");
    std::fs::write(dir.path().join("a.txt"), "hit one\nmiss\nhit two\n").expect("write");
    let found = grep_in(&dir, &json!({"pattern": "hit", "output_mode": "content", "-n": true}));
    assert_eq!(found, "a.txt\n1:hit one\n3:hit two\n\nb.txt\n1:hit three");
}

#[test]
fn a_long_line_is_cut_and_says_how_much_was_left_out() {
    let dir = tempfile::tempdir().expect("dir");
    std::fs::write(dir.path().join("min.js"), format!("hit{}\n", "x".repeat(1_000))).expect("write");
    let found = grep_in(&dir, &json!({"pattern": "hit", "output_mode": "content"}));
    assert_eq!(found, format!("min.js\nhit{}… [+703 characters]", "x".repeat(297)));
}

#[test]
fn one_file_shows_its_first_matches_and_counts_the_rest() {
    let dir = tempfile::tempdir().expect("dir");
    std::fs::write(dir.path().join("log.txt"), "hit\n".repeat(30)).expect("write");
    let found = grep_in(&dir, &json!({"pattern": "hit", "output_mode": "content"}));
    assert_eq!(found, format!("log.txt\n{}… [5 more matches in this file]", "hit\n".repeat(25)));
}

#[test]
fn results_stop_at_a_default_limit_that_head_limit_replaces() {
    let dir = tempfile::tempdir().expect("dir");
    for at in 0..205 {
        std::fs::write(dir.path().join(format!("f{at}.txt")), "hit\n").expect("write");
    }
    let listed = grep_in(&dir, &json!({"pattern": "hit"}));
    assert_eq!(listed.lines().count(), 201);
    assert!(listed.ends_with("\n… [5 more results; narrow the search or raise head_limit]"), "{listed}");
    assert_eq!(grep_in(&dir, &json!({"pattern": "hit", "head_limit": 300})).lines().count(), 205);
    let two = grep_in(&dir, &json!({"pattern": "hit", "head_limit": 2}));
    assert!(two.ends_with("\n… [203 more results; narrow the search or raise head_limit]"), "{two}");
}

#[test]
fn grep_reports_no_match_and_bad_patterns() {
    let (_dir, workspace) = grep_workspace();
    assert_eq!(grep(&json!({"pattern": "absent"}), &workspace).content, "No matches found.");
    let bad = grep(&json!({"pattern": "("}), &workspace);
    assert!(bad.is_error && bad.content.starts_with("Invalid pattern"), "{bad:?}");
    let glob = grep(&json!({"pattern": "run", "glob": "[" }), &workspace);
    assert!(glob.is_error && glob.content.starts_with("Invalid glob"), "{glob:?}");
}

#[test]
fn a_multiline_match_spans_lines_and_shows_every_line_it_covers() {
    let dir = tempfile::tempdir().expect("dir");
    std::fs::write(dir.path().join("a.rs"), "fn start() {\n    middle();\n}\nfn other() {}\n").expect("write");
    let spanning = json!({"pattern": "start.*\\n.*middle", "output_mode": "content", "-n": true});
    assert_eq!(grep_in(&dir, &spanning), "No matches found.", "without multiline a match stays on its line");
    let mut multiline = spanning.clone();
    multiline["multiline"] = json!(true);
    assert_eq!(grep_in(&dir, &multiline), "a.rs\n1:fn start() {\n2:    middle();");
    let dotted = json!({"pattern": "start.*\\}", "multiline": true, "output_mode": "count"});
    assert_eq!(grep_in(&dir, &dotted), "a.rs:1", "a dot crosses lines and the match runs to the last brace");
    let per_line = json!({"pattern": "\\(\\)", "output_mode": "count"});
    assert_eq!(grep_in(&dir, &per_line), "a.rs:3");
}

#[test]
fn the_file_tools_declare_what_they_touch() {
    let dir = tempfile::tempdir().expect("dir");
    let workspace = Workspace::new(dir.path());
    let effects = |name: &str, input: serde_json::Value| shipped(name).effects(&input, &workspace);
    assert_eq!(effects("Read", json!({"file_path": "a"})), vec![Effect::ReadPath(workspace.place("a"))]);
    assert_eq!(effects("Read", json!({})), Vec::<Effect>::new());
    assert_eq!(effects("Write", json!({"file_path": "a"})), vec![Effect::WritePath(workspace.place("a"))]);
    assert_eq!(effects("Edit", json!({})), vec![Effect::Declared(crate::tools::Declared::Edits)]);
    assert_eq!(effects("NotebookEdit", json!({"notebook_path": "n"})), vec![Effect::WritePath(workspace.place("n"))]);
    assert_eq!(effects("Grep", json!({"pattern": "x"})), vec![Effect::ReadTree(workspace.place("."))]);
    assert_eq!(shipped("Grep").summary(&json!({"pattern": "x"})), "x");
    assert_eq!(shipped("Edit").summary(&json!({"file_path": "f.rs"})), "f.rs");
    assert!(shipped("Read").spec.description.contains("Returns up to 2000 lines"));
    assert!(shipped("Grep").spec.description.contains("first 25 matches"));
}

#[test]
fn artifact_publishes_the_written_file_and_names_it_for_the_chat() {
    let dir = tempfile::tempdir().expect("dir");
    let shelf = tempfile::tempdir().expect("shelf");
    std::fs::write(dir.path().join("q3.html"), "<title>Vendas Q3</title><p>ok</p>").expect("page");
    let mut workspace = Workspace::new(dir.path());
    workspace.session_id = "s-9".into();
    workspace.services.artifacts = Some(shelf.path().to_path_buf());
    let artifact = shipped("Artifact");
    assert!(artifact.spec.description.contains("artifact-design"), "{}", artifact.spec.description);
    assert_eq!(artifact.summary(&json!({ "file_path": "q3.html", "title": " Q3 " })), "Q3");

    let outcome = artifact.run(&json!({ "file_path": "q3.html" }), &workspace);

    assert!(!outcome.is_error, "{}", outcome.content);
    assert!(outcome.content.starts_with("Published \"Vendas Q3\" (page)."), "{}", outcome.content);
    let id = domain::tools::artifact::id_in(&outcome.content).expect("an id");
    assert!(shelf.path().join(id).join("q3.html").is_file());
    let refused = artifact.run(&json!({ "file_path": "missing.html" }), &workspace);
    assert!(refused.is_error, "{}", refused.content);
}
