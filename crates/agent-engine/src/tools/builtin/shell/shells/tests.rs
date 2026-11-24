use std::process::Stdio;
use std::time::Duration;

use serde_json::json;

use super::*;
use crate::tools::Tool;

pub(crate) fn of_client(client: u64) -> Workspace {
    let mut workspace = Workspace::new(std::env::temp_dir());
    workspace.credentials.client = client;
    workspace
}

fn workspace(client: u64) -> Workspace {
    of_client(client)
}

#[test]
fn stopping_a_command_this_agent_started_needs_no_permission_and_any_other_asks() {
    use crate::tools::Effect;
    let id = start(&of_client(908), "server", &mut sh("sleep 30")).expect("start");
    let acts = |id: &str, client| {
        let kill = crate::tools::scripted_tests::shipped("KillShell");
        kill.effects(&json!({ "shell_id": id }), &workspace(client)).iter().any(Effect::acts)
    };
    assert!(!acts(&id, 908), "its own command");
    assert!(acts(&id, 909), "another agent's command");
    assert!(acts("bash_nope", 908), "a command it never started");
    assert!(kill(908, &id));
}

fn sh(script: &str) -> std::process::Command {
    let mut command = std::process::Command::new("/bin/sh");
    command.arg("-c").arg(script).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    command
}

#[test]
fn stdout_and_stderr_keep_the_order_they_were_printed_in() {
    let (stop, detach) = (std::sync::atomic::AtomicBool::new(false), std::sync::atomic::AtomicBool::new(false));
    let id = start(&of_client(905), "both", &mut sh("echo one; echo two >&2; echo three")).expect("start");
    let Waited::Exited { output, .. } = wait(&id, Duration::from_secs(120), &stop, &detach) else { panic!("ended") };
    assert_eq!(output, "one\ntwo\nthree");
}

#[path = "lifecycle_tests.rs"]
mod lifecycle;
