#![cfg(unix)]
#![allow(clippy::expect_used, reason = "a failed setup step stops the test")]

use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::process::Command;
use std::sync::Arc;

use platform::confine::{Ceiling, Rules};

const FIRST: u32 = 300_000;

fn said(mut command: Command) -> (bool, String) {
    let done = command.output().expect("ran");
    (done.status.success(), String::from_utf8_lossy(&done.stdout).trim().to_owned())
}

fn shell(code: &str, owner: Option<&str>) -> Command {
    let mut command = Command::new("/bin/sh");
    command.arg("-c").arg(code).current_dir("/");
    platform::confine::as_owner(&mut command, owner).expect("confined");
    command
}

#[test]
fn each_person_runs_as_their_own_user_and_reaches_nothing_of_the_server_or_of_the_others() {
    if platform::may_confine().is_err() {
        eprintln!("skipped: confining agents needs root");
        return;
    }
    let place = tempfile::tempdir().expect("place");
    std::fs::set_permissions(place.path(), std::fs::Permissions::from_mode(0o711)).expect("passable");
    let rules = Rules {
        first_uid: FIRST,
        uids: 10,
        homes: place.path().join("homes"),
        kept: vec!["PATH".into()],
        ceiling: Ceiling { processes: 64, ..Ceiling::default() },
    };
    let numbers = |owner: &str| {
        Ok(match owner {
            "ana" => 1,
            "bia" => 2,
            _ => 0,
        })
    };
    platform::confine::activate(rules, Arc::new(numbers)).expect("activated");
    let secret = place.path().join("secret.key");
    std::fs::write(&secret, "key").expect("secret");

    assert_eq!(said(shell("id -u", Some("ana"))), (true, (FIRST + 1).to_string()));
    assert_eq!(said(shell("id -u", None)), (true, FIRST.to_string()));
    assert!(std::env::var_os("CARGO_MANIFEST_DIR").is_some());
    assert_eq!(said(shell("echo \"[$CARGO_MANIFEST_DIR]\"", Some("ana"))), (true, "[]".into()));
    assert!(!said(shell(&format!("cat {}", secret.display()), Some("ana"))).0);
    assert_eq!(said(shell("grep 'Max processes' /proc/self/limits | tr -s ' ' | cut -d' ' -f3", Some("ana"))).1, "64");

    let anas = place.path().join("workspaces").join("ana");
    platform::confine::passable_dir(&place.path().join("workspaces")).expect("workspaces");
    platform::confine::dir_for(&anas, Some("ana")).expect("ana's folder");
    assert!(said(shell(&format!("touch {}/mine", anas.display()), Some("ana"))).0);
    assert!(!said(shell(&format!("ls {}", anas.display()), Some("bia"))).0);

    let written = anas.join("deep").join("by-the-server.txt");
    platform::atomic::write(&written, b"text").expect("written by the server");
    assert_eq!(std::fs::metadata(&written).expect("file").uid(), FIRST + 1);
    assert_eq!(std::fs::metadata(anas.join("deep")).expect("dir").uid(), FIRST + 1);
    assert!(said(shell(&format!("echo more >> {}", written.display()), Some("ana"))).0);

    let mut git = platform::git::command(&anas);
    git.arg("init").arg("--quiet");
    assert!(git.status().expect("git").success());
    assert_eq!(std::fs::metadata(anas.join(".git")).expect("repo").uid(), FIRST + 1);

    let tools = place.path().join("closed").join("language-servers");
    std::fs::create_dir_all(tools.join("bin")).expect("tools");
    std::fs::write(tools.join("bin").join("server"), "#!/bin/sh\necho served\n").expect("server");
    std::fs::set_permissions(tools.join("bin").join("server"), std::fs::Permissions::from_mode(0o700)).expect("mode");
    std::fs::set_permissions(place.path().join("closed"), std::fs::Permissions::from_mode(0o700)).expect("closed");
    let run_tool = || said(shell(&tools.join("bin").join("server").display().to_string(), Some("ana")));
    assert!(!run_tool().0);
    platform::confine::open_to_all(&tools).expect("opened");
    assert_eq!(run_tool(), (true, "served".into()));
    assert!(!said(shell(&format!("ls {}", place.path().join("closed").display()), Some("ana"))).0);

    let old = place.path().join("workspaces").join("bia");
    std::fs::create_dir_all(old.join("repo")).expect("old folder");
    assert!(!platform::confine::belongs_to(&old, Some("bia")).expect("checked"));
    platform::confine::give_tree(&old, Some("bia")).expect("given");
    assert!(platform::confine::belongs_to(&old.join("repo"), Some("bia")).expect("checked"));
    assert!(!platform::confine::belongs_to(&anas, Some("bia")).expect("checked"));

    let mut lsp = Command::new("id");
    lsp.arg("-u");
    platform::confine::confined_in(&mut lsp, &anas).expect("in ana's folder");
    assert_eq!(said(lsp).1, (FIRST + 1).to_string());
    let mut elsewhere = Command::new("id");
    elsewhere.arg("-u");
    platform::confine::confined_in(&mut elsewhere, place.path()).expect("in the server's folder");
    assert_eq!(said(elsewhere).1, FIRST.to_string());
}
