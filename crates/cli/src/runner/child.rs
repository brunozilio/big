use std::ffi::OsString;
use std::path::Path;
use std::process::Stdio;
use std::sync::Arc;

use serde_json::json;
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWriteExt, BufReader};
use tokio::process::Command;
use tokio::sync::mpsc;

use super::Runner;
use crate::discovered::{self, Discovered};

fn follow(reader: impl AsyncRead + Unpin + Send + 'static, runner: Arc<Runner>) {
    tokio::spawn(async move {
        let mut lines = BufReader::new(reader).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            runner.said(&line);
        }
    });
}

pub fn start(
    command: &Discovered,
    args: Vec<OsString>,
    root: &Path,
    runner: Arc<Runner>,
    mut answers: mpsc::Receiver<String>,
) -> Result<(), String> {
    let (program, fixed) = discovered::program(command)?;
    let exe = std::env::current_exe().map_err(|error| error.to_string())?;
    let mut child = Command::new(&program)
        .args(fixed)
        .args(args)
        .current_dir(&command.folder)
        .env("COMMAND_EXE", exe)
        .env("COMMAND_NAME", brand::COMMAND)
        .env("COMMAND_ROOT", root)
        .env("COMMAND_PAGE", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("{}: {error}", program.display()))?;
    let (Some(mut stdin), Some(stdout), Some(stderr)) = (child.stdin.take(), child.stdout.take(), child.stderr.take())
    else {
        return Err("the command's pipes could not be opened".to_owned());
    };
    follow(stdout, Arc::clone(&runner));
    follow(stderr, Arc::clone(&runner));
    tokio::spawn(async move {
        while let Some(answer) = answers.recv().await {
            let line = format!("{}\n", answer.replace('\n', " "));
            if stdin.write_all(line.as_bytes()).await.is_err() {
                break;
            }
        }
    });
    tokio::spawn(async move {
        let code = child.wait().await.ok().and_then(|status| status.code());
        runner.tell(&json!({ "type": "exit", "code": code }));
    });
    Ok(())
}
