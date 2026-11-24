use serde_json::json;

use super::*;
use crate::tools::{Outcome, Tool};

fn git(dir: &Path, args: &[&str]) {
    let done = std::process::Command::new("git").args(args).current_dir(dir).output().expect("git");
    assert!(done.status.success(), "git {args:?}");
}

fn fixture() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("dir");
    let root = dir.path();
    git(root, &["init", "-q"]);
    let files: [(&str, &[u8]); 15] = [
        (".gitignore", b"ignored.txt\nbuild/\n"),
        (".rgignore", b"skipped.txt\n"),
        ("ignored.txt", b"hit ignored\n"),
        ("skipped.txt", b"hit skipped\n"),
        ("build/out.txt", b"hit built\n"),
        (".hidden/secret.txt", b"hit hidden\nHIT loud\n"),
        (".env", b"TOKEN=hit\n"),
        ("src/a.rs", b"fn start() {\n    hit();\n}\n\nfn other() { hit(); hit(); }\n"),
        ("src/b.md", b"Hit it\nmiss\nmiss\nmiss\nhit again\n"),
        ("src/deep/c.rs", b"// HIT\nlet start = 1;\nlet end = 2;\n"),
        ("src/nul.dat", b"hit\0before a nul\n"),
        ("src/latin1.txt", b"hit \xe9t\xe9\n"),
        ("src/a-b.txt", b"hit dash\n"),
        ("a/z.txt", b"hit z\n"),
        ("big.log", b"one hit\ntwo\nthree hit\nfour\nfive\nsix hit\n"),
    ];
    for (name, bytes) in files {
        let path = root.join(name);
        std::fs::create_dir_all(path.parent().expect("parent")).expect("folder");
        std::fs::write(path, bytes).expect("write");
    }
    std::fs::create_dir_all(root.join(".git/info")).expect("info");
    std::fs::write(root.join(".git/info/exclude"), "excluded.txt\n").expect("exclude");
    std::fs::write(root.join("excluded.txt"), "hit excluded\n").expect("excluded");
    dir
}

fn questions() -> Vec<Value> {
    vec![
        json!({"pattern": "hit"}),
        json!({"pattern": "hit", "-i": true}),
        json!({"pattern": "hit", "glob": "*.rs"}),
        json!({"pattern": "hit", "glob": "src/**"}),
        json!({"pattern": "hit", "path": "src"}),
        json!({"pattern": "hit", "path": "src/b.md"}),
        json!({"pattern": "hit", "output_mode": "count"}),
        json!({"pattern": "hit", "output_mode": "count", "-i": true}),
        json!({"pattern": "hit", "output_mode": "content", "-n": true}),
        json!({"pattern": "hit", "output_mode": "content", "-C": 1}),
        json!({"pattern": "hit", "output_mode": "content", "-n": true, "-A": 2, "-B": 1}),
        json!({"pattern": "hit", "head_limit": 3}),
        json!({"pattern": "^fn \\w+", "output_mode": "content"}),
        json!({"pattern": "start.*\\}", "multiline": true}),
        json!({"pattern": "start.*\\}", "multiline": true, "output_mode": "count"}),
        json!({"pattern": "start\\s+let", "multiline": true, "output_mode": "content", "-n": true}),
        json!({"pattern": "start.*end", "multiline": true, "-i": true, "output_mode": "content"}),
        json!({"pattern": "absent"}),
        json!({"pattern": "befo\\x{0}"}),
        json!({"pattern": "a\\nb"}),
        json!({"pattern": "nul", "output_mode": "content"}),
    ]
}

fn grep_with(engine: Engine, input: &Value, workspace: &crate::tools::Workspace) -> Outcome {
    FORCED.with(|forced| *forced.borrow_mut() = Some(engine));
    let outcome = crate::tools::scripted_tests::shipped("Grep").run(input, workspace);
    FORCED.with(|forced| *forced.borrow_mut() = None);
    outcome
}

#[test]
fn the_built_in_search_skips_ignored_files_and_git_reads_hidden_ones_and_orders_by_path() {
    let dir = fixture();
    let workspace = crate::tools::Workspace::new(dir.path());
    let found = grep_with(Engine::BuiltIn, &json!({"pattern": "hit", "-i": true}), &workspace).content;
    let found: Vec<&str> = found.lines().collect();
    assert_eq!(
        found,
        [
            ".env",
            ".hidden/secret.txt",
            "a/z.txt",
            "big.log",
            "src/a-b.txt",
            "src/a.rs",
            "src/b.md",
            "src/deep/c.rs",
            "src/nul.dat"
        ]
    );
}

#[test]
fn ripgrep_and_the_built_in_search_give_the_same_answers() {
    let Some(rg) = ripgrep::program() else {
        eprintln!("rg is not on PATH: only the built-in search was checked");
        return;
    };
    let dir = fixture();
    let workspace = crate::tools::Workspace::new(dir.path());
    let asked = ripgrep::Asked { pattern: "hit", ignore_case: false, multiline: false };
    let listed = ripgrep::candidates(&rg, dir.path(), &asked, &workspace.stop).expect("ripgrep answered");
    let names: Vec<String> = listed.iter().map(|path| shown(path, &workspace).replace('\\', "/")).collect();
    assert!(names.contains(&"src/a.rs".to_owned()) && names.contains(&".env".to_owned()), "{names:?}");
    for left_out in ["ignored.txt", "skipped.txt", "excluded.txt", "build/out.txt"] {
        assert!(!names.contains(&left_out.to_owned()), "{left_out} in {names:?}");
    }
    for asked in questions() {
        let quick = grep_with(Engine::Ripgrep(rg.clone()), &asked, &workspace);
        let slow = grep_with(Engine::BuiltIn, &asked, &workspace);
        assert_eq!(quick, slow, "{asked}");
    }
}

#[test]
fn ripgrep_is_found_on_the_path_and_asked_for_the_files_that_could_match() {
    let dir = tempfile::tempdir().expect("dir");
    let name = if cfg!(windows) { "rg.exe" } else { "rg" };
    std::fs::write(dir.path().join(name), "").expect("rg");
    let path = std::env::join_paths([dir.path().join("missing"), dir.path().to_path_buf()]).expect("path");
    assert_eq!(ripgrep::on_path(Some(&path)), Some(dir.path().join(name)));
    assert_eq!(ripgrep::on_path(Some(dir.path().join("missing").as_os_str())), None);
    assert_eq!(ripgrep::on_path(None), None);

    let asked = ripgrep::Asked { pattern: "-x", ignore_case: true, multiline: true };
    let arguments: Vec<String> = ripgrep::arguments(Path::new("/repo"), &asked)
        .into_iter()
        .map(|argument| argument.to_string_lossy().into_owned())
        .collect();
    assert!(arguments.starts_with(&["--no-config".to_owned(), "--files-with-matches".to_owned()]));
    for wanted in ["--hidden", "--text", "--ignore-case", "--multiline", "--multiline-dotall", "--glob=!.git"] {
        assert!(arguments.iter().any(|argument| argument == wanted), "{wanted}");
    }
    assert!(arguments.ends_with(&["--regexp".into(), "-x".into(), "--".into(), "/repo".into()]));
}

#[cfg(unix)]
#[test]
fn a_ripgrep_that_fails_leaves_the_search_to_the_built_in_engine() {
    use std::os::unix::fs::PermissionsExt;
    let dir = fixture();
    let fake = tempfile::tempdir().expect("fake");
    let program = fake.path().join("rg");
    std::fs::write(&program, "#!/bin/sh\nexit 2\n").expect("script");
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).expect("mode");
    let workspace = crate::tools::Workspace::new(dir.path());
    let asked = json!({"pattern": "hit", "path": "src"});
    assert_eq!(grep_with(Engine::Ripgrep(program), &asked, &workspace), grep_with(Engine::BuiltIn, &asked, &workspace));
}

#[test]
fn the_search_stays_inside_the_tree_it_declared() {
    let dir = fixture();
    let workspace = crate::tools::Workspace::new(dir.path());
    let effects = [crate::tools::Effect::ReadTree(workspace.place("src"))];
    let reach = Reach { workspace: &workspace, fence: super::super::Fence::of(&effects), streams: None };
    assert!(call(&reach, "grep", &json!({ "pattern": "hit" })).is_err());
    let asked = json!({ "pattern": "hit", "path": "src/deep", "mode": "count", "ignoreCase": true });
    let found = call(&reach, "grep", &asked).expect("grep");
    assert_eq!(found, json!([{ "path": "src/deep/c.rs", "count": 1 }]));
}
