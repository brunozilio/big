mod child;
mod proxy;
mod routes;

use std::ffi::OsString;
use std::path::Path;
use std::process::ExitCode;
use std::sync::{Arc, Mutex, PoisonError, RwLock};

use serde_json::{Value, json};
use tokio::sync::{broadcast, mpsc};

use crate::discovered::Discovered;

pub const KEY_COOKIE: &str = "runner_key";

#[derive(Debug)]
pub struct Runner {
    history: Mutex<Vec<String>>,
    live: broadcast::Sender<String>,
    input: mpsc::Sender<String>,
    remote: RwLock<Option<String>>,
    key: String,
}

impl Runner {
    pub fn tell(&self, event: &Value) {
        if event["type"] == "connect"
            && let Some(url) = event["url"].as_str()
            && let Ok(mut remote) = self.remote.write()
        {
            *remote = Some(url.trim_end_matches('/').to_owned());
        }
        let line = event.to_string();
        self.history.lock().unwrap_or_else(PoisonError::into_inner).push(line.clone());
        let _ = self.live.send(line);
    }

    pub fn said(&self, line: &str) {
        match serde_json::from_str::<Value>(line) {
            Ok(event) if event.get("type").is_some_and(Value::is_string) => self.tell(&event),
            _ => self.tell(&json!({ "type": "log", "text": line })),
        }
    }

    pub fn so_far(&self) -> (Vec<String>, broadcast::Receiver<String>) {
        let history = self.history.lock().unwrap_or_else(PoisonError::into_inner);
        (history.clone(), self.live.subscribe())
    }

    pub fn remote(&self) -> Option<String> {
        self.remote.read().ok().and_then(|remote| remote.clone())
    }

    pub fn key(&self) -> &str {
        &self.key
    }

    pub async fn answer(&self, answer: String) -> bool {
        self.input.send(answer).await.is_ok()
    }
}

fn new_key() -> Result<String, String> {
    platform::crypto::random(24).map(|bytes| platform::crypto::hex(&bytes))
}

pub fn run(command: &Discovered, args: Vec<OsString>, root: &Path) -> Result<ExitCode, String> {
    let runtime =
        tokio::runtime::Builder::new_multi_thread().enable_all().build().map_err(|error| error.to_string())?;
    runtime.block_on(serve(command, args, root))
}

async fn serve(command: &Discovered, args: Vec<OsString>, root: &Path) -> Result<ExitCode, String> {
    let (input, answers) = mpsc::channel(16);
    let runner = Arc::new(Runner {
        history: Mutex::new(Vec::new()),
        live: broadcast::channel(1024).0,
        input,
        remote: RwLock::new(None),
        key: new_key()?,
    });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.map_err(|error| error.to_string())?;
    let port = listener.local_addr().map_err(|error| error.to_string())?.port();
    child::start(command, args, root, Arc::clone(&runner), answers)?;
    let url = format!("http://127.0.0.1:{port}/run?key={}", runner.key());
    println!("{} is open at {url}", command.name);
    println!("keep this terminal open; Ctrl+C ends it");
    crate::open_in_browser(&url);
    let app = routes::router(runner, port);
    let stop = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    axum::serve(listener, app).with_graceful_shutdown(stop).await.map_err(|error| error.to_string())?;
    Ok(ExitCode::SUCCESS)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn runner() -> Runner {
        Runner {
            history: Mutex::new(Vec::new()),
            live: broadcast::channel(8).0,
            input: mpsc::channel(1).0,
            remote: RwLock::new(None),
            key: "k".into(),
        }
    }

    #[test]
    fn plain_lines_become_log_events_and_a_connect_event_sets_the_remote() {
        let runner = runner();
        runner.said("building...");
        runner.said(r#"{"type":"connect","url":"https://x.example.com/app/"}"#);
        runner.said(r#"{"no":"type"}"#);
        let (history, _) = runner.so_far();
        assert_eq!(history[0], json!({ "type": "log", "text": "building..." }).to_string());
        assert_eq!(runner.remote().as_deref(), Some("https://x.example.com/app"));
        assert!(history[2].contains("\"type\":\"log\""));
    }
}
