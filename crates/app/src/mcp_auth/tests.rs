use std::sync::Arc;

use agent_engine::model::http::ScriptedHttp;
use serde_json::json;

use super::*;

const SERVER: &str = "https://mcp.test/mcp";

fn tokens_for(server: &str) -> McpTokens {
    McpTokens {
        access_token: format!("a-{server}"),
        refresh_token: None,
        expires_at: None,
        client_id: "c".into(),
        client_secret: None,
        token_endpoint: "https://auth.test/token".into(),
        resource: server.to_owned(),
    }
}

#[test]
fn two_processes_signing_in_to_different_servers_keep_both_sign_ins() {
    let dir = tempfile::tempdir().expect("dir");
    let file = dir.path().join("secrets.json");
    let servers: Vec<String> = (0..8).map(|n| format!("https://mcp{n}.test/mcp")).collect();
    let start = std::sync::Barrier::new(servers.len());
    std::thread::scope(|scope| {
        for server in &servers {
            let store = crate::secrets::SealedSecretStore::open(&file, [7; 32]).expect("store");
            let auth = McpAuth::new(Arc::new(store), Arc::new(ScriptedHttp::new([])));
            let start = &start;
            scope.spawn(move || {
                start.wait();
                auth.save(server, Some(&tokens_for(server))).expect("saved");
            });
        }
    });
    let store = crate::secrets::SealedSecretStore::open(&file, [7; 32]).expect("store");
    let reader = McpAuth::new(Arc::new(store), Arc::new(ScriptedHttp::new([])));
    let kept: Vec<&String> = servers.iter().filter(|server| reader.token(server).is_some()).collect();
    assert_eq!(kept.len(), servers.len(), "kept only {kept:?}");
}

#[derive(Debug)]
struct SingleUseRefreshTokens {
    valid: Mutex<String>,
    issued: std::sync::atomic::AtomicUsize,
    refused: std::sync::atomic::AtomicUsize,
}

impl Http for SingleUseRefreshTokens {
    fn post(
        &self,
        _url: &str,
        _headers: &[(String, String)],
        body: &str,
    ) -> Result<agent_engine::model::http::Response, String> {
        use std::sync::atomic::Ordering;
        let presented =
            body.split('&').find_map(|pair| pair.strip_prefix("refresh_token=")).unwrap_or_default().to_owned();
        std::thread::sleep(std::time::Duration::from_millis(300));
        let mut valid = self.valid.lock().expect("valid token");
        let (status, answer) = if presented == *valid {
            let next = self.issued.fetch_add(1, Ordering::SeqCst) + 2;
            *valid = format!("r-{next}");
            (200, json!({ "access_token": format!("a-{next}"), "refresh_token": *valid, "expires_in": 3600 }))
        } else {
            self.refused.fetch_add(1, Ordering::SeqCst);
            (400, json!({ "error": "invalid_grant" }))
        };
        Ok(agent_engine::model::http::Response {
            status,
            headers: Vec::new(),
            body: agent_engine::model::http::Body::from(answer.to_string()),
        })
    }

    fn get(&self, _url: &str, _headers: &[(String, String)]) -> Result<agent_engine::model::http::Response, String> {
        Err("not used".to_owned())
    }
}

#[test]
fn two_processes_with_a_stale_mcp_sign_in_refresh_it_once_and_both_get_a_token() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let dir = tempfile::tempdir().expect("dir");
    let stale = McpTokens {
        access_token: "a-1".to_owned(),
        refresh_token: Some("r-1".to_owned()),
        expires_at: Some(1_000),
        client_id: "c-1".to_owned(),
        client_secret: None,
        token_endpoint: "https://auth.test/token".to_owned(),
        resource: SERVER.to_owned(),
    };
    let endpoint = Arc::new(SingleUseRefreshTokens {
        valid: Mutex::new("r-1".to_owned()),
        issued: AtomicUsize::new(0),
        refused: AtomicUsize::new(0),
    });
    McpAuth::new(crate::secrets::secrets_in(Some(dir.path())), endpoint.clone())
        .save(SERVER, Some(&stale))
        .expect("signed in");
    let start = Arc::new(std::sync::Barrier::new(2));

    let processes: Vec<_> = (0..2)
        .map(|_| {
            let auth = McpAuth::new(crate::secrets::secrets_in(Some(dir.path())), endpoint.clone()).at(5_000);
            let start = Arc::clone(&start);
            std::thread::spawn(move || {
                start.wait();
                auth.token(SERVER)
            })
        })
        .collect();
    let tokens: Vec<_> = processes.into_iter().map(|process| process.join().expect("process")).collect();

    assert_eq!(tokens, [Some("a-2".to_owned()), Some("a-2".to_owned())], "neither process lost the sign-in");
    assert_eq!(endpoint.refused.load(Ordering::SeqCst), 0, "no refresh token was presented twice");
    assert_eq!(endpoint.issued.load(Ordering::SeqCst), 1, "the second process used the first one's new token");
    let kept = McpAuth::new(crate::secrets::secrets_in(Some(dir.path())), endpoint.clone()).load(SERVER);
    assert_eq!(kept.and_then(|tokens| tokens.refresh_token).as_deref(), Some("r-2"));
    let leftovers: Vec<_> = std::fs::read_dir(dir.path())
        .expect("dir")
        .filter_map(Result::ok)
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.contains("refresh"))
        .collect();
    assert!(leftovers.is_empty(), "the refresh lock leaves nothing behind: {leftovers:?}");
}

#[test]
fn each_server_refreshes_under_its_own_lock() {
    assert_ne!(refresh_slot(SERVER), refresh_slot("https://other.test/mcp"));
    assert_eq!(refresh_slot(SERVER), refresh_slot(SERVER));
}
